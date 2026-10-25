use crate::organizer::sanitize_species_slug;
use crate::scanner::decompose_filename;
use crate::types::AssetBundle;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Constructs an in-place formatted filename:
/// `<stem>_<Species_Common_Name>[_<YYYYMMDD>][_<Location_Slug>].<ext>`
/// Date and location are cleanly omitted when absent or empty (no sentinels).
pub fn format_inplace_filename(
    stem: &str,
    species: &str,
    date: Option<&str>,
    location: Option<&str>,
    ext: &str,
) -> String {
    let clean_species = species.trim_matches('_');
    let species_slug = sanitize_species_slug(clean_species);

    let mut date_segment = None;
    if let Some(d) = date {
        let trimmed = d.trim();
        let lower = trimmed.to_lowercase();
        if !trimmed.is_empty() && lower != "unknown" && lower != "unknowndate" {
            let normalized = trimmed.replace(['-', ':', '/'], "");
            if !normalized.is_empty() {
                date_segment = Some(normalized);
            }
        }
    }

    let mut loc_segment = None;
    if let Some(l) = location {
        let trimmed = l.trim();
        let lower = trimmed.to_lowercase();
        if !trimmed.is_empty() && lower != "unknown" && lower != "unknownlocation" {
            let clean_loc = sanitize_species_slug(trimmed.trim_matches('_'));
            if !clean_loc.is_empty() {
                loc_segment = Some(clean_loc);
            }
        }
    }

    let mut parts = Vec::new();
    if !species_slug.is_empty() {
        parts.push(species_slug);
    }
    if let Some(d) = date_segment {
        parts.push(d);
    }
    if let Some(l) = loc_segment {
        parts.push(l);
    }

    let full_stem = if parts.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}_{}", parts.join("_"))
    };

    if ext.is_empty() {
        full_stem
    } else if ext.starts_with('.') {
        format!("{full_stem}{ext}")
    } else {
        format!("{full_stem}.{ext}")
    }
}

/// Idempotency guard: checks if the stem already contains the species slug
/// to prevent double-suffixing on repeated runs.
pub fn is_already_inplace_named(stem: &str, species: &str) -> bool {
    let clean_species = species.trim_matches('_');
    let species_slug = sanitize_species_slug(clean_species);
    if species_slug.is_empty() {
        return false;
    }

    stem.contains(&format!("_{species_slug}"))
        || stem.ends_with(&species_slug)
        || stem == species_slug
        || stem.contains(&species_slug)
}

/// Renames the primary image, companion images, and sidecar files in lockstep
/// in their original location on disk.
/// Skips renaming if `is_already_inplace_named` detects the stem is already suffixed.
/// Resolves disk collisions by appending `_1`, `_2` before extensions.
/// Returns the list of `(original_path, new_path)` pairs for manifest logging.
pub fn execute_inplace_bundle_rename(
    bundle: &AssetBundle,
    species: &str,
    date: Option<&str>,
    location: Option<&str>,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    if is_already_inplace_named(&bundle.original_stem, species) {
        return Ok(Vec::new());
    }

    let derived_date = date.map(|d| d.to_string()).or_else(|| {
        bundle.capture_timestamp_ms.and_then(|ms| {
            chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.format("%Y%m%d").to_string())
        })
    });
    let effective_date = derived_date.as_deref();
    let effective_loc = location.or(bundle.resolved_location_name.as_deref());

    let new_base_stem = format_inplace_filename(
        &bundle.original_stem,
        species,
        effective_date,
        effective_loc,
        "",
    );

    execute_inplace_bundle_rename_with_stem(bundle, &new_base_stem)
}

/// Renames all files in an asset bundle using an explicit base stem.
pub fn execute_inplace_bundle_rename_with_stem(
    bundle: &AssetBundle,
    new_stem: &str,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    if bundle.original_stem == new_stem {
        return Ok(Vec::new());
    }

    let mut source_files = Vec::new();
    let mut seen_canonical = HashSet::new();

    let mut add_file = |path: &Path| {
        if path.exists() {
            if let Ok(canon) = path.canonicalize() {
                if seen_canonical.insert(canon) {
                    source_files.push(path.to_path_buf());
                }
            } else if seen_canonical.insert(path.to_path_buf()) {
                source_files.push(path.to_path_buf());
            }
        }
    };

    add_file(&bundle.primary_image);
    for comp in &bundle.companion_images {
        add_file(comp);
    }
    for sidecar in &bundle.sidecar_files {
        add_file(sidecar);
    }

    // Discover any additional sidecar files sharing the same original stem on disk
    if let Some(parent) = bundle.primary_image.parent() {
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                        if let Some((stem, _)) = decompose_filename(file_name) {
                            if stem == bundle.original_stem {
                                add_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    if source_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut renames_plan = Vec::new();
    for src_path in &source_files {
        let file_name = src_path
            .file_name()
            .and_then(|f| f.to_str())
            .context("File has invalid UTF-8 filename")?;

        let remainder = if let Some((base_stem, _)) = decompose_filename(file_name) {
            if file_name.len() >= base_stem.len() {
                file_name[base_stem.len()..].to_string()
            } else {
                String::new()
            }
        } else if let Some(pos) = file_name.find('.') {
            file_name[pos..].to_string()
        } else {
            String::new()
        };

        renames_plan.push((src_path.clone(), remainder));
    }

    // Search for disambiguated base stem if target already exists on disk
    let mut counter = 0;
    let final_base_stem = loop {
        let candidate_stem = if counter == 0 {
            new_stem.to_string()
        } else {
            format!("{new_stem}_{counter}")
        };

        let has_collision = renames_plan.iter().any(|(src, rem)| {
            if let Some(parent) = src.parent() {
                let target = parent.join(format!("{candidate_stem}{rem}"));
                if target.exists() {
                    if let Ok(canon_target) = target.canonicalize() {
                        !seen_canonical.contains(&canon_target)
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            }
        });

        if !has_collision {
            break candidate_stem;
        }
        counter += 1;
    };

    let mut completed_renames = Vec::new();
    for (src, rem) in &renames_plan {
        let parent = src.parent().unwrap_or_else(|| Path::new("."));
        let dst = parent.join(format!("{final_base_stem}{rem}"));

        if *src == dst {
            continue;
        }

        if let Err(e) = fs::rename(src, &dst) {
            // Roll back renames that succeeded earlier in this batch
            for (original_src, current_dst) in completed_renames.iter().rev() {
                let _ = fs::rename(current_dst, original_src);
            }
            return Err(e).with_context(|| {
                format!("Failed to rename {} to {}", src.display(), dst.display())
            });
        }

        completed_renames.push((src.clone(), dst));
    }

    Ok(completed_renames)
}
