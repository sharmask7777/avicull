//! # Reconciling Manual Finder/Disk Moves into Durable Database State
//!
//! Photographers frequently drag-and-drop photos in macOS Finder to correct species misclassifications.
//!
//! Rather than overwriting manual user moves on subsequent runs, `avicull` treats Finder moves as
//! authoritative first-class curation events:
//! - Reconciles hard links to bundles via APFS inode parity (`meta.ino()`).
//! - Detects when a bundle has been relocated to a different species folder and records an immutable user override (`is_user_override = 1`) in SQLite.
//! - Subsequent runs preserve the user's manual sorting and propagate it to all companion files and sidecars.

use crate::db::{ExistingBundleMap, ManifestDb};
use crate::organizer::inplace::{match_known_category, parse_inplace_filename};
use crate::organizer::sanitize_species_slug;
use crate::scanner::decompose_filename;
use crate::types::{AssetCategory, RoutingTier};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use walkdir::WalkDir;

/// Reconciles manual filesystem moves within the output directory into durable
/// SQLite classification overrides and synchronized hard link operation records.
///
/// Scans the output directory tree, matches hard links to existing bundles via
/// APFS inode parity and filename stems, detects if bundles have been moved into
/// different species directories, and records permanent user overrides.
pub fn reconcile_manual_disk_moves(
    output_root: &Path,
    db: &ManifestDb,
    existing_bundle_map: &mut ExistingBundleMap,
) -> Result<usize> {
    if !output_root.exists() || existing_bundle_map.is_empty() {
        return Ok(0);
    }

    let can_output = output_root.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize output root: {}",
            output_root.display()
        )
    })?;

    let mut reconciled_bundle_ids = HashSet::new();

    let walker = WalkDir::new(&can_output)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') || name == "@eaDir" || name.ends_with(".photoslibrary") {
                return false;
            }
            if entry.path() != can_output && name == "birds_hierarchy" {
                return false;
            }
            true
        });

    for entry_res in walker {
        let Ok(entry) = entry_res else {
            continue;
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path().to_path_buf();
        let file_name = entry.file_name().to_string_lossy();

        if file_name.starts_with('.')
            || file_name == "catalog.html"
            || file_name == "bird_catalog.html"
            || file_name == "apply_corrections.sh"
        {
            continue;
        }

        // Only process recognized media or sidecar files
        if decompose_filename(&file_name).is_none() {
            continue;
        }

        let Ok(rel_path) = file_path.strip_prefix(&can_output) else {
            continue;
        };

        // Determine target species slug and routing tier from directory path components
        let dir_components: Vec<&str> = rel_path
            .parent()
            .map(|p| p.iter().filter_map(|c| c.to_str()).collect())
            .unwrap_or_default();

        if dir_components.is_empty() {
            // File is placed directly in output root, not inside a species folder
            continue;
        }

        let (target_tier, target_slug, target_category) = if dir_components[0] == "_Needs_Review" {
            if dir_components.len() > 1 {
                let slug =
                    sanitize_species_slug(dir_components.last().unwrap_or(&dir_components[1]));
                let cat = if dir_components.len() > 2 {
                    match_known_category(dir_components[1])
                        .unwrap_or_else(|| AssetCategory::from_slug(&slug))
                } else {
                    AssetCategory::from_slug(&slug)
                };
                (RoutingTier::NeedsReview, slug, cat)
            } else {
                continue;
            }
        } else if dir_components[0] == "_No_Bird_Detected"
            || dir_components[0] == "_No_Subject_Detected"
        {
            (
                RoutingTier::NoBirdDetected,
                "_No_Bird_Detected".to_string(),
                AssetCategory::NoSubjectDetected,
            )
        } else if let Some(species_dir) = dir_components.last() {
            let slug = sanitize_species_slug(species_dir);
            let cat = if dir_components.len() > 1 {
                match_known_category(dir_components[0])
                    .unwrap_or_else(|| AssetCategory::from_slug(&slug))
            } else {
                AssetCategory::from_slug(&slug)
            };
            (RoutingTier::ConfirmedSpecies, slug, cat)
        } else {
            continue;
        };

        // Match the file to a known bundle
        let mut matched_bundle_id: Option<String> = None;
        let mut file_ino = 0u64;

        if let Ok(meta) = entry.metadata() {
            file_ino = meta.ino();
            if file_ino != 0 {
                if let Some((_, b)) = existing_bundle_map.by_inode.get(&file_ino) {
                    matched_bundle_id = Some(b.bundle_id.clone());
                } else if let Ok(Some(b_id)) = db.get_bundle_id_by_inode(file_ino) {
                    matched_bundle_id = Some(b_id);
                }
            }
        }

        if matched_bundle_id.is_none() {
            // Stem matching fallback when inodes differ
            let (trip_prefix, rest) = if let Some(idx) = file_name.find("__") {
                (Some(&file_name[..idx]), &file_name[idx + 2..])
            } else {
                (None, file_name.as_ref())
            };

            let mut best_stem_match: Option<(usize, String)> = None;
            for (b_id, (_, b)) in &existing_bundle_map.by_id {
                if let Some(prefix) = trip_prefix {
                    if sanitize_species_slug(&b.trip_folder_name) != prefix
                        && b.trip_folder_name != prefix
                    {
                        continue;
                    }
                }
                let stem = &b.original_stem;
                let is_match = rest == stem
                    || rest.starts_with(&format!("{stem}."))
                    || rest.starts_with(&format!("{stem}_"));
                if is_match
                    && best_stem_match
                        .as_ref()
                        .is_none_or(|(len, _)| *len < stem.len())
                {
                    best_stem_match = Some((stem.len(), b_id.clone()));
                }
            }
            if let Some((_, b_id)) = best_stem_match {
                matched_bundle_id = Some(b_id);
            }
        }

        let Some(bundle_id) = matched_bundle_id else {
            continue;
        };

        if let Some((existing_decision, _)) = existing_bundle_map.get(&bundle_id).cloned() {
            // An item residing in _No_Bird_Detected / _No_Subject_Detected is only a manual override
            // if the user deliberately demoted it from ConfirmedSpecies or NeedsReview.
            // If it was already in NoBirdDetected, it was routed automatically and must not be marked as a user override.
            let is_override = if target_tier == RoutingTier::NoBirdDetected {
                existing_decision.routing_tier != RoutingTier::NoBirdDetected
            } else {
                let is_species_diff = existing_decision.assigned_species_slug != target_slug;
                let is_promoted = existing_decision.routing_tier == RoutingTier::NeedsReview
                    && target_tier == RoutingTier::ConfirmedSpecies;
                let is_tier_diff = existing_decision.routing_tier != target_tier;
                is_species_diff
                    || is_promoted
                    || (is_tier_diff && !existing_decision.is_user_override)
            };

            if is_override {
                db.update_bundle_override_with_category(
                    &bundle_id,
                    target_category.as_str(),
                    &target_slug,
                    target_tier,
                    1.0,
                )?;

                let mut updated_dec = existing_decision.clone();
                updated_dec.assigned_species_slug.clone_from(&target_slug);
                updated_dec.routing_tier = target_tier;
                updated_dec.final_confidence = 1.0;
                updated_dec.is_user_override = true;
                existing_bundle_map.update_decision(&bundle_id, updated_dec);

                reconciled_bundle_ids.insert(bundle_id.clone());
            }

            // Synchronize hardlink operations record with the new file destination
            let updated_by_inode = if file_ino != 0 {
                db.update_hardlink_destination_by_inode(&bundle_id, file_ino, &file_path)
                    .unwrap_or(false)
            } else {
                false
            };

            if !updated_by_inode {
                let _ =
                    db.update_hardlink_destination_by_filename(&bundle_id, &file_name, &file_path);
            }
        }
    }

    Ok(reconciled_bundle_ids.len())
}

/// Purges bundles from the database and in-memory map if their assets no longer
/// exist on disk in the input tree.
pub fn prune_missing_bundles_from_manifest(
    discovered_bundles: &[crate::types::AssetBundle],
    db: &ManifestDb,
    existing_bundle_map: &mut ExistingBundleMap,
) -> Result<usize> {
    if existing_bundle_map.is_empty() {
        return Ok(0);
    }

    let mut surviving_bundle_ids = HashSet::new();
    for bundle in discovered_bundles {
        if let Some((_, old_bundle)) = existing_bundle_map.get_by_bundle(bundle) {
            surviving_bundle_ids.insert(old_bundle.bundle_id.clone());
        }
    }

    let all_existing_ids: Vec<String> = existing_bundle_map.by_id.keys().cloned().collect();
    let mut pruned_count = 0;

    for id in all_existing_ids {
        if !surviving_bundle_ids.contains(&id) {
            let _ = db.delete_bundle(&id);
            existing_bundle_map.remove(&id);
            pruned_count += 1;
        }
    }

    Ok(pruned_count)
}

/// Extracts the species slug and routing tier from an in-place filename stem.
///
/// Handles 2-level nomenclature `<original_stem>_<Category>_<Subject>[_<Date>][_<Location>]`,
/// legacy v1 single-level stems `<original_stem>_<Species>[_<Date>][_<Location>]`,
/// `Needs_Review_<Species>`, `_No_Bird_Detected`, and `_No_Subject_Detected`.
pub(crate) fn parse_inplace_slug_and_tier(
    file_stem: &str,
    original_stem: &str,
    known_location: Option<&str>,
) -> Option<(AssetCategory, String, RoutingTier)> {
    if let Some((cat, subject, tier, _date, loc)) = parse_inplace_filename(file_stem, original_stem)
    {
        if tier == RoutingTier::NoBirdDetected {
            return Some((
                AssetCategory::NoSubjectDetected,
                "_No_Bird_Detected".to_string(),
                RoutingTier::NoBirdDetected,
            ));
        }

        let mut final_subject = subject;
        if loc.is_none() {
            if let Some(known_loc) = known_location {
                let loc_slug = sanitize_species_slug(known_loc);
                if !loc_slug.is_empty() {
                    let suffix_pattern = format!("_{loc_slug}");
                    if let Some(stripped) = final_subject.strip_suffix(&suffix_pattern) {
                        final_subject = stripped.to_string();
                    }
                }
            }
        }

        let slug = sanitize_species_slug(&final_subject);
        if slug.is_empty() {
            Some((
                AssetCategory::NoSubjectDetected,
                "_No_Bird_Detected".to_string(),
                RoutingTier::NoBirdDetected,
            ))
        } else {
            let effective_cat = if cat == AssetCategory::NoSubjectDetected {
                AssetCategory::from_slug(&slug)
            } else {
                cat
            };
            Some((effective_cat, slug, tier))
        }
    } else {
        None
    }
}

/// Reconciles manual in-place filename renames within the operating tree into durable
/// SQLite classification overrides and synchronizes companion image files and sidecars.
///
/// When running in in-place mode, users can manually rename image files (e.g.
/// `DSC_0001_Mallard.JPG` -> `DSC_0001_Northern_Pintail.JPG` or with date/location suffixes).
/// This function detects the change, updates the bundle decision in SQLite to the new species
/// with `is_user_override = true`, renames all remaining companion and sidecar files to match,
/// and updates the in-memory `ExistingBundleMap`.
pub fn reconcile_inplace_manual_renames(
    operating_root: &Path,
    db: &ManifestDb,
    existing_bundle_map: &mut ExistingBundleMap,
) -> Result<usize> {
    if !operating_root.exists() || existing_bundle_map.is_empty() {
        return Ok(0);
    }

    let can_operating_root = operating_root.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize operating root: {}",
            operating_root.display()
        )
    })?;

    let mut reconciled_bundle_ids = HashSet::new();

    let walker = WalkDir::new(&can_operating_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') || name == "@eaDir" || name.ends_with(".photoslibrary") {
                return false;
            }
            if entry.path() != can_operating_root && name == "birds_hierarchy" {
                return false;
            }
            true
        });

    for entry_res in walker {
        let Ok(entry) = entry_res else {
            continue;
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path().to_path_buf();
        let file_name = entry.file_name().to_string_lossy();

        if file_name.starts_with('.')
            || file_name == "catalog.html"
            || file_name == "bird_catalog.html"
            || file_name == "apply_corrections.sh"
        {
            continue;
        }

        let Some((file_stem, _is_sidecar)) = decompose_filename(&file_name) else {
            continue;
        };

        // Match the file to a known bundle via inode or stem matching
        let mut matched_bundle_id: Option<String> = None;
        let mut file_ino = 0u64;

        if let Ok(meta) = entry.metadata() {
            file_ino = meta.ino();
            if file_ino != 0 {
                if let Some((_, b)) = existing_bundle_map.by_inode.get(&file_ino) {
                    matched_bundle_id = Some(b.bundle_id.clone());
                } else if let Ok(Some(b_id)) = db.get_bundle_id_by_inode(file_ino) {
                    matched_bundle_id = Some(b_id);
                }
            }
        }

        if matched_bundle_id.is_none() {
            let mut best_stem_match: Option<(usize, String)> = None;
            for (b_id, (_, b)) in &existing_bundle_map.by_id {
                let stem = &b.original_stem;
                let is_match = file_stem == *stem
                    || file_stem.starts_with(&format!("{stem}_"))
                    || file_stem.starts_with(&format!("{stem}."));
                if is_match
                    && best_stem_match
                        .as_ref()
                        .is_none_or(|(len, _)| *len < stem.len())
                {
                    best_stem_match = Some((stem.len(), b_id.clone()));
                }
            }
            if let Some((_, b_id)) = best_stem_match {
                matched_bundle_id = Some(b_id);
            }
        }

        let Some(bundle_id) = matched_bundle_id else {
            continue;
        };

        if reconciled_bundle_ids.contains(&bundle_id) {
            continue;
        }

        let Some((existing_decision, bundle)) = existing_bundle_map.get(&bundle_id).cloned() else {
            continue;
        };

        let Some((target_cat, target_slug, target_tier)) = parse_inplace_slug_and_tier(
            &file_stem,
            &bundle.original_stem,
            bundle.resolved_location_name.as_deref(),
        ) else {
            continue;
        };

        let is_override = if target_tier == RoutingTier::NoBirdDetected {
            existing_decision.routing_tier != RoutingTier::NoBirdDetected
        } else {
            let is_species_diff = existing_decision.assigned_species_slug != target_slug;
            let is_promoted = existing_decision.routing_tier == RoutingTier::NeedsReview
                && target_tier == RoutingTier::ConfirmedSpecies;
            let is_tier_diff = existing_decision.routing_tier != target_tier;
            is_species_diff || is_promoted || (is_tier_diff && !existing_decision.is_user_override)
        };

        if is_override {
            db.update_bundle_override_with_category(
                &bundle_id,
                target_cat.as_str(),
                &target_slug,
                target_tier,
                1.0,
            )?;

            let mut updated_dec = existing_decision.clone();
            updated_dec.assigned_species_slug.clone_from(&target_slug);
            updated_dec.routing_tier = target_tier;
            updated_dec.final_confidence = 1.0;
            updated_dec.is_user_override = true;
            existing_bundle_map.update_decision(&bundle_id, updated_dec);

            let new_base_stem = file_stem.clone();

            if file_ino != 0 {
                let _ = db.update_inplace_destination_by_inode(&bundle_id, file_ino, &file_path);
            }
            let _ = db.update_inplace_destination_by_filename(&bundle_id, &file_name, &file_path);

            // Reconcile and rename companion images and sidecars in the same directory
            if let Some(parent_dir) = file_path.parent() {
                if let Ok(entries) = fs::read_dir(parent_dir) {
                    for comp_entry_res in entries.flatten() {
                        let comp_path = comp_entry_res.path();
                        if !comp_path.is_file() {
                            continue;
                        }
                        let comp_fname = comp_entry_res.file_name().to_string_lossy().to_string();
                        if comp_fname.starts_with('.')
                            || comp_fname == "catalog.html"
                            || comp_fname == "bird_catalog.html"
                            || comp_fname == "apply_corrections.sh"
                        {
                            continue;
                        }

                        if let Some((comp_stem, _)) = decompose_filename(&comp_fname) {
                            let belongs_to_bundle = comp_stem == bundle.original_stem
                                || comp_stem.starts_with(&format!("{}_", bundle.original_stem));

                            if belongs_to_bundle
                                && comp_path != file_path
                                && comp_stem != new_base_stem
                            {
                                let remainder = if comp_fname.len() >= comp_stem.len() {
                                    &comp_fname[comp_stem.len()..]
                                } else {
                                    ""
                                };
                                let target_comp_name = format!("{new_base_stem}{remainder}");
                                let target_comp_path = parent_dir.join(&target_comp_name);

                                if comp_path != target_comp_path
                                    && !target_comp_path.exists()
                                    && fs::rename(&comp_path, &target_comp_path).is_ok()
                                {
                                    if let Ok(cmeta) = fs::metadata(&target_comp_path) {
                                        let c_ino = cmeta.ino();
                                        if c_ino != 0 {
                                            let _ = db.update_inplace_destination_by_inode(
                                                &bundle_id,
                                                c_ino,
                                                &target_comp_path,
                                            );
                                        }
                                    }
                                    let _ = db.update_inplace_destination_by_filename(
                                        &bundle_id,
                                        &comp_fname,
                                        &target_comp_path,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            reconciled_bundle_ids.insert(bundle_id.clone());
        }
    }

    Ok(reconciled_bundle_ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inplace_slug_and_tier_confirmed() {
        let (cat, slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Northern_Pintail", "DSC_0001", None).unwrap();
        assert_eq!(cat, AssetCategory::Bird);
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_with_date_and_location() {
        let (cat, slug, tier) = parse_inplace_slug_and_tier(
            "DSC_0001_Northern_Pintail_20261025_Leh",
            "DSC_0001",
            Some("Leh"),
        )
        .unwrap();
        assert_eq!(cat, AssetCategory::Bird);
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_with_year() {
        let (cat, slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Northern_Pintail_2024", "DSC_0001", None)
                .unwrap();
        assert_eq!(cat, AssetCategory::Bird);
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_needs_review() {
        let (cat, slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Needs_Review_Northern_Pintail", "DSC_0001", None)
                .unwrap();
        assert_eq!(cat, AssetCategory::Bird);
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::NeedsReview);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_no_bird_detected() {
        let (cat, slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001__No_Bird_Detected", "DSC_0001", None).unwrap();
        assert_eq!(cat, AssetCategory::NoSubjectDetected);
        assert_eq!(slug, "_No_Bird_Detected");
        assert_eq!(tier, RoutingTier::NoBirdDetected);

        assert_eq!(
            parse_inplace_slug_and_tier("DSC_0001", "DSC_0001", None),
            None
        );
    }
}
