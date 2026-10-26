pub mod correct;
pub mod inplace;
pub mod reconcile;
pub use correct::*;
pub use inplace::*;
pub use reconcile::*;

use crate::db::{HardlinkOperation, ManifestDb};
use crate::scanner::decompose_filename;
use crate::types::{AssetBundle, BundleDecision, RoutingTier};
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Sanitizes species common names into directory slugs:
/// Replaces spaces with underscores, strips apostrophes, and replaces non-alphanumerics
/// (except '-' and '_') with '_'.
pub fn sanitize_species_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '\'' {
            // Strip apostrophes
            continue;
        } else if c == ' ' {
            out.push('_');
        } else if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.to_string()
}

/// Constructs formatted destination filename:
/// <TripPrefix__><OriginalStem>_<Date>_<Location>_<Confidence:0.2>.<OriginalExt>
/// Preserves compound sidecar extensions (e.g. DSC_4011.JPG.xmp -> DSC_4011_2025-01-26_Saul_Kere_0.96.JPG.xmp).
pub fn format_destination_filename(
    original_filename: &str,
    confidence: f32,
    trip_prefix: Option<&str>,
    capture_date: Option<&str>,
    inferred_location: Option<&str>,
) -> String {
    let conf_str = format!("{:.2}", confidence);

    let (stem, remainder) = if let Some((base_stem, _)) = decompose_filename(original_filename) {
        // Find remainder after base_stem
        let rem = if original_filename.len() >= base_stem.len() {
            &original_filename[base_stem.len()..]
        } else {
            ""
        };
        (base_stem, rem.to_string())
    } else {
        // Fallback: split on first dot
        if let Some(pos) = original_filename.find('.') {
            (
                original_filename[..pos].to_string(),
                original_filename[pos..].to_string(),
            )
        } else {
            (original_filename.to_string(), String::new())
        }
    };

    let mut tags = Vec::new();
    if let Some(d) = capture_date {
        let trimmed = d.trim();
        if !trimmed.is_empty() {
            tags.push(trimmed);
        }
    }
    if let Some(loc) = inferred_location {
        let trimmed = loc.trim_matches(|c: char| c.is_whitespace() || c == '_');
        if !trimmed.is_empty() {
            tags.push(trimmed);
        }
    }

    let base_stem_with_tags = if tags.is_empty() {
        format!("{stem}_{conf_str}")
    } else {
        let tags_str = tags.join("_");
        format!("{stem}_{tags_str}_{conf_str}")
    };

    let filename = format!("{base_stem_with_tags}{remainder}");

    if let Some(prefix) = trip_prefix {
        format!("{prefix}__{filename}")
    } else {
        filename
    }
}

/// Computes the target destination directory inside output_root based on routing tier.
pub fn compute_target_dir(output_root: &Path, decision: &BundleDecision) -> PathBuf {
    match decision.routing_tier {
        RoutingTier::ConfirmedSpecies => output_root.join(&decision.assigned_species_slug),
        RoutingTier::NeedsReview => output_root
            .join("_Needs_Review")
            .join(&decision.assigned_species_slug),
        RoutingTier::NoBirdDetected => output_root.join("_No_Bird_Detected"),
    }
}

/// Materializes POSIX hard links for an AssetBundle and its companion and sidecar files.
/// Resolves collisions deterministically and handles re-run idempotency.
pub fn materialize_bundle_hardlinks(
    bundle: &AssetBundle,
    decision: &BundleDecision,
    output_root: &Path,
    run_id: &str,
    dry_run: bool,
) -> Result<Vec<HardlinkOperation>> {
    let target_dir = compute_target_dir(output_root, decision);

    if !dry_run && !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .with_context(|| format!("Failed to create directory: {}", target_dir.display()))?;
    }

    // Collect all files in bundle (primary + companions + sidecars)
    let mut files_to_link = Vec::new();
    files_to_link.push(bundle.primary_image.clone());
    for comp in &bundle.companion_images {
        files_to_link.push(comp.clone());
    }
    for sidecar in &bundle.sidecar_files {
        files_to_link.push(sidecar.clone());
    }

    let capture_date = bundle.capture_timestamp_ms.and_then(|ms| {
        chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.format("%Y-%m-%d").to_string())
    });

    let inferred_location = bundle
        .resolved_location_name
        .as_deref()
        .map(sanitize_species_slug);

    // First check if primary file has a collision requiring trip prefix
    let primary_filename = bundle
        .primary_image
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.jpg".to_string());

    let normal_dst_filename = format_destination_filename(
        &primary_filename,
        decision.final_confidence,
        None,
        capture_date.as_deref(),
        inferred_location.as_deref(),
    );
    let normal_dst_path = target_dir.join(&normal_dst_filename);

    let needs_trip_prefix = if normal_dst_path.exists() {
        if let Ok(meta) = fs::metadata(&normal_dst_path) {
            meta.ino() != bundle.source_inode
        } else {
            false
        }
    } else {
        false
    };

    let trip_slug = if needs_trip_prefix {
        Some(sanitize_species_slug(&bundle.trip_folder_name))
    } else {
        None
    };

    let mut operations = Vec::new();
    let now = Utc::now().to_rfc3339();

    for src_path in files_to_link {
        let src_filename = src_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let dst_filename = format_destination_filename(
            &src_filename,
            decision.final_confidence,
            trip_slug.as_deref(),
            capture_date.as_deref(),
            inferred_location.as_deref(),
        );

        let mut dst_path = target_dir.join(dst_filename);
        let src_meta = fs::metadata(&src_path)
            .with_context(|| format!("Failed to stat source file: {}", src_path.display()))?;
        let src_ino = src_meta.ino();

        let mut status = "planned".to_string();
        let mut counter = 1;

        let base_stem = dst_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let base_ext = dst_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());

        while dst_path.exists() {
            if let Ok(dst_meta) = fs::metadata(&dst_path) {
                if dst_meta.ino() == src_ino {
                    status = "already_linked".to_string();
                    break;
                } else {
                    // Different file with same name: append clean numbered counter from base
                    let new_name = match &base_ext {
                        Some(e) => format!("{}_{}.{}", base_stem, counter, e),
                        None => format!("{}_{}", base_stem, counter),
                    };
                    dst_path = target_dir.join(new_name);
                    counter += 1;
                }
            } else {
                break;
            }
        }

        if !dry_run && status != "already_linked" {
            // Create hard link
            fs::hard_link(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to create hard link from '{}' to '{}'",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
            status = "linked".to_string();
        }

        operations.push(HardlinkOperation {
            op_id: None,
            run_id: run_id.to_string(),
            bundle_id: bundle.bundle_id.clone(),
            source_path: src_path,
            dest_path: dst_path,
            source_inode: src_ino,
            status,
            operation_type: "hardlink".to_string(),
            executed_at: Some(now.clone()),
        });
    }

    Ok(operations)
}

/// Unlinks generated hard links or rolls back in-place renames for a given run ID,
/// leaving input files completely untouched and restoring original filenames.
pub fn clean_run_operations(db: &ManifestDb, run_id: &str) -> Result<usize> {
    let ops = db.get_run_operations(run_id)?;
    let mut cleaned_count = 0usize;
    let mut affected_dirs = Vec::new();

    // Process operations in reverse order
    for op in ops.into_iter().rev() {
        if op.operation_type == "in_place_rename" {
            if op.status == "renamed" {
                if op.dest_path == op.source_path {
                    continue;
                }
                if op.dest_path.exists() {
                    fs::rename(&op.dest_path, &op.source_path).with_context(|| {
                        format!(
                            "Failed to revert rename '{}' -> '{}'",
                            op.dest_path.display(),
                            op.source_path.display()
                        )
                    })?;
                    cleaned_count += 1;
                }
                if let Some(id) = op.op_id {
                    let _ = db.update_operation_status(id, "reverted");
                }
            }
        } else {
            // Hardlink operation
            if op.status == "linked" || op.status == "already_linked" {
                // Defense-in-depth safety checks:
                // 1. Never unlink if destination path equals source path
                if op.dest_path == op.source_path {
                    continue;
                }
                if op.dest_path.exists() {
                    // Safety check: ensure destination inode matches source inode
                    if let Ok(meta) = fs::metadata(&op.dest_path) {
                        if meta.ino() == op.source_inode {
                            fs::remove_file(&op.dest_path).with_context(|| {
                                format!("Failed to unlink '{}'", op.dest_path.display())
                            })?;
                            cleaned_count += 1;
                            if let Some(parent) = op.dest_path.parent() {
                                affected_dirs.push(parent.to_path_buf());
                            }
                        }
                    }
                }
                if let Some(id) = op.op_id {
                    let _ = db.update_operation_status(id, "unlinked");
                }
            }
        }
    }

    // Clean up empty directories: sort descending by path component depth
    // so children directories are removed before parent directories
    affected_dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));
    affected_dirs.dedup();
    for d in affected_dirs {
        if d.exists() {
            let _ = fs::remove_dir(&d);
        }
    }

    Ok(cleaned_count)
}

/// Unlinks all generated hard links in output directory for a given run ID,
/// leaving input files completely untouched. Also safely rolls back any in-place renames.
pub fn clean_run_hardlinks(db: &ManifestDb, run_id: &str) -> Result<usize> {
    clean_run_operations(db, run_id)
}

/// Reverts all in-place renames for a given run ID.
pub fn clean_run_inplace(db: &ManifestDb, run_id: &str) -> Result<usize> {
    db.rollback_inplace_renames(run_id)
}
