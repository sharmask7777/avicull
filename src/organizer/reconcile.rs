use crate::db::{ExistingBundleMap, ManifestDb};
use crate::organizer::sanitize_species_slug;
use crate::scanner::decompose_filename;
use crate::types::RoutingTier;
use anyhow::{Context, Result};
use std::collections::HashSet;
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
