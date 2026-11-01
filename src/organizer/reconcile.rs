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
use crate::organizer::sanitize_species_slug;
use crate::scanner::decompose_filename;
use crate::types::RoutingTier;
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
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
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

        let rel_path = match file_path.strip_prefix(&can_output) {
            Ok(p) => p,
            Err(_) => continue,
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

        let (target_tier, target_slug) = if dir_components[0] == "_Needs_Review" {
            if dir_components.len() > 1 {
                (
                    RoutingTier::NeedsReview,
                    sanitize_species_slug(dir_components[1]),
                )
            } else {
                continue;
            }
        } else if dir_components[0] == "_No_Bird_Detected" {
            (RoutingTier::NoBirdDetected, "_No_Bird_Detected".to_string())
        } else {
            // Direct species directory under output root (or nested hierarchy)
            let species_dir = dir_components.last().unwrap();
            (
                RoutingTier::ConfirmedSpecies,
                sanitize_species_slug(species_dir),
            )
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
                        .map(|(len, _)| *len < stem.len())
                        .unwrap_or(true)
                {
                    best_stem_match = Some((stem.len(), b_id.clone()));
                }
            }
            if let Some((_, b_id)) = best_stem_match {
                matched_bundle_id = Some(b_id);
            }
        }

        let bundle_id = match matched_bundle_id {
            Some(id) => id,
            None => continue,
        };

        if let Some((existing_decision, _)) = existing_bundle_map.get(&bundle_id).cloned() {
            let is_species_diff = existing_decision.assigned_species_slug != target_slug;
            let is_promoted = existing_decision.routing_tier == RoutingTier::NeedsReview
                && target_tier == RoutingTier::ConfirmedSpecies;
            let is_tier_diff = existing_decision.routing_tier != target_tier;

            if is_species_diff
                || is_promoted
                || (is_tier_diff && !existing_decision.is_user_override)
            {
                db.update_bundle_override(&bundle_id, &target_slug, target_tier, 1.0)?;

                let mut updated_dec = existing_decision.clone();
                updated_dec.assigned_species_slug = target_slug.clone();
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
/// Handles stems formatted as `<original_stem>_<Species>[_<Date>][_<Location>]`,
/// `Needs_Review_<Species>`, `_No_Bird_Detected`, and bare original stems.
pub(crate) fn parse_inplace_slug_and_tier(
    file_stem: &str,
    original_stem: &str,
    known_location: Option<&str>,
) -> Option<(String, RoutingTier)> {
    if file_stem == original_stem {
        return None;
    }

    let suffix = if file_stem.len() > original_stem.len() && file_stem.starts_with(original_stem) {
        let rest = &file_stem[original_stem.len()..];
        rest.trim_start_matches('_')
    } else {
        file_stem.trim_start_matches('_')
    };

    if suffix.is_empty()
        || suffix == "_No_Bird_Detected"
        || suffix == "No_Bird_Detected"
        || suffix == "NoBirdDetected"
    {
        return Some(("_No_Bird_Detected".to_string(), RoutingTier::NoBirdDetected));
    }

    let (tier, remainder) = if let Some(stripped) = suffix.strip_prefix("Needs_Review_") {
        (RoutingTier::NeedsReview, stripped)
    } else if let Some(stripped) = suffix.strip_prefix("NeedsReview_") {
        (RoutingTier::NeedsReview, stripped)
    } else {
        (RoutingTier::ConfirmedSpecies, suffix)
    };

    let mut tokens: Vec<&str> = remainder.split('_').filter(|s| !s.is_empty()).collect();

    let strip_location = |toks: &mut Vec<&str>| {
        if let Some(loc) = known_location {
            let loc_slug = sanitize_species_slug(loc);
            if !loc_slug.is_empty() {
                if let Some(last) = toks.last() {
                    if last.eq_ignore_ascii_case(&loc_slug) {
                        toks.pop();
                        return true;
                    }
                }
            }
        }
        false
    };

    let strip_date = |toks: &mut Vec<&str>| {
        if let Some(last) = toks.last() {
            let is_numeric_date =
                (last.len() == 4 || last.len() == 8) && last.chars().all(|c| c.is_ascii_digit());
            if is_numeric_date {
                toks.pop();
                return true;
            }
        }
        false
    };

    for _ in 0..2 {
        if !strip_location(&mut tokens) {
            strip_date(&mut tokens);
        }
    }

    let species_raw = tokens.join("_");
    let species_slug = sanitize_species_slug(&species_raw);

    if species_slug.is_empty() {
        Some(("_No_Bird_Detected".to_string(), RoutingTier::NoBirdDetected))
    } else {
        Some((species_slug, tier))
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
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
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

        let (file_stem, _is_sidecar) = match decompose_filename(&file_name) {
            Some(res) => res,
            None => continue,
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
                        .map(|(len, _)| *len < stem.len())
                        .unwrap_or(true)
                {
                    best_stem_match = Some((stem.len(), b_id.clone()));
                }
            }
            if let Some((_, b_id)) = best_stem_match {
                matched_bundle_id = Some(b_id);
            }
        }

        let bundle_id = match matched_bundle_id {
            Some(id) => id,
            None => continue,
        };

        if reconciled_bundle_ids.contains(&bundle_id) {
            continue;
        }

        let (existing_decision, bundle) = match existing_bundle_map.get(&bundle_id).cloned() {
            Some(pair) => pair,
            None => continue,
        };

        let (target_slug, target_tier) = match parse_inplace_slug_and_tier(
            &file_stem,
            &bundle.original_stem,
            bundle.resolved_location_name.as_deref(),
        ) {
            Some(res) => res,
            None => continue,
        };

        let is_species_diff = existing_decision.assigned_species_slug != target_slug;
        let is_promoted = existing_decision.routing_tier == RoutingTier::NeedsReview
            && target_tier == RoutingTier::ConfirmedSpecies;
        let is_tier_diff = existing_decision.routing_tier != target_tier;

        if is_species_diff || is_promoted || (is_tier_diff && !existing_decision.is_user_override) {
            db.update_bundle_override(&bundle_id, &target_slug, target_tier, 1.0)?;

            let mut updated_dec = existing_decision.clone();
            updated_dec.assigned_species_slug = target_slug.clone();
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
        let (slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Northern_Pintail", "DSC_0001", None).unwrap();
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_with_date_and_location() {
        let (slug, tier) = parse_inplace_slug_and_tier(
            "DSC_0001_Northern_Pintail_20261025_Leh",
            "DSC_0001",
            Some("Leh"),
        )
        .unwrap();
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_with_year() {
        let (slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Northern_Pintail_2024", "DSC_0001", None)
                .unwrap();
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_needs_review() {
        let (slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001_Needs_Review_Northern_Pintail", "DSC_0001", None)
                .unwrap();
        assert_eq!(slug, "Northern_Pintail");
        assert_eq!(tier, RoutingTier::NeedsReview);
    }

    #[test]
    fn test_parse_inplace_slug_and_tier_no_bird_detected() {
        let (slug, tier) =
            parse_inplace_slug_and_tier("DSC_0001__No_Bird_Detected", "DSC_0001", None).unwrap();
        assert_eq!(slug, "_No_Bird_Detected");
        assert_eq!(tier, RoutingTier::NoBirdDetected);

        assert_eq!(
            parse_inplace_slug_and_tier("DSC_0001", "DSC_0001", None),
            None
        );
    }
}
