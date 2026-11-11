//! # Hierarchy Organization, APFS Hard Links & Name Formatting
//!
//! Handles destination tree generation via POSIX hard links:
//! - Materializes zero-byte links pointing to original source inodes (`st_ino`).
//! - Encodes trip, capture date, location, and confidence into descriptive filenames.
//! - Converts species common names into cross-platform filesystem slugs.
//! - Routes assets into `Species_Slug/`, `_Needs_Review/Species_Slug/`, or `_No_Bird_Detected/`.

pub mod batch;
pub mod chunk;
pub mod correct;
pub mod engine;
pub mod inplace;
pub mod reconcile;
pub use crate::catalog::DEFAULT_CATALOG_FILENAME;
pub use batch::*;
pub use chunk::*;
pub use correct::*;
pub use engine::*;
pub use inplace::*;
pub use reconcile::*;

use crate::catalog::CatalogItem;
use crate::db::{HardlinkOperation, ManifestDb};
use crate::scanner::decompose_filename;
use crate::storage::{StorageBackend, StorageKind};
use crate::types::{
    AssetBundle, AssetCategory, AvicullError, BundleDecision, OrganizationMode, RoutingTier,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Validates that the requested organization mode is supported by the storage backend.
/// Rejects `--mode hierarchy` on non-local storage backends (CloudStorage / NetworkShare).
pub fn validate_organization_mode_for_backend(
    backend: &dyn StorageBackend,
    mode: OrganizationMode,
) -> Result<()> {
    if mode == OrganizationMode::HierarchicalHardlinkHierarchy
        && backend.kind() != StorageKind::LocalVolume
    {
        let storage_desc = match backend.kind() {
            StorageKind::MacOSCloudStorage => "cloud-synchronized storage",
            StorageKind::NetworkShare => "network shares (SMB/NFS)",
            StorageKind::LocalVolume => "local storage",
        };
        return Err(AvicullError::HierarchyModeUnsupportedOnRemote {
            storage_type: storage_desc,
        }
        .into());
    }
    Ok(())
}

/// Sanitizes species common names into directory slugs:
/// Replaces spaces with underscores, strips apostrophes, and replaces non-alphanumerics
/// (except '-' and '_') with '_'.
pub fn sanitize_species_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '\'' {
            // Strip apostrophes
            continue;
        }
        if c == ' ' {
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
/// `<TripPrefix__><OriginalStem>_<Date>_<Location>_<Confidence:0.2>.<OriginalExt>`
/// Preserves compound sidecar extensions (e.g. `DSC_4011.JPG.xmp` -> `DSC_4011_2025-01-26_Saul_Kere_0.96.JPG.xmp`).
pub fn format_destination_filename(
    original_filename: &str,
    confidence: f32,
    trip_prefix: Option<&str>,
    capture_date: Option<&str>,
    inferred_location: Option<&str>,
) -> String {
    let conf_str = format!("{confidence:.2}");

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

/// Constructs the destination directory path for an asset in hierarchy mode.
///
/// Hierarchy paths follow the singular multi-taxa schema:
/// - [`RoutingTier::ConfirmedSpecies`] -> `<output_root>/<Category>/<SubjectSlug>`
/// - [`RoutingTier::NeedsReview`] -> `<output_root>/_Needs_Review/<SubjectSlug>`
/// - [`RoutingTier::NoBirdDetected`] (or [`RoutingTier::NoSubjectDetected`]) -> `<output_root>/_No_Subject_Detected`
///
/// # Arguments
/// * `output_root` - Root hierarchy directory (e.g. `birds_hierarchy`).
/// * `category` - Singular taxonomic or genre category (e.g. `"Bird"`, `"Mammalia"`).
/// * `subject_slug` - Sanitized subject or species directory slug (e.g. `"White_throated_Kingfisher"`).
/// * `routing_tier` - Destination sorting tier ([`RoutingTier`]).
///
/// # Returns
/// A [`PathBuf`] pointing to the target directory.
#[must_use]
pub fn build_output_destination_path(
    output_root: &Path,
    category: &str,
    subject_slug: &str,
    routing_tier: RoutingTier,
) -> PathBuf {
    let cat = if category.trim().is_empty() {
        "Bird"
    } else {
        category.trim()
    };
    let clean_slug = subject_slug.trim();
    let effective_subject = if let Some(stripped) = clean_slug.strip_prefix(&format!("{cat}_")) {
        stripped
    } else if let Some(stripped) = clean_slug.strip_prefix(&format!("{cat}/")) {
        stripped
    } else {
        clean_slug
    };

    match routing_tier {
        RoutingTier::ConfirmedSpecies => output_root.join(cat).join(effective_subject),
        RoutingTier::NeedsReview => output_root
            .join(RoutingTier::DIR_NEEDS_REVIEW)
            .join(effective_subject),
        RoutingTier::NoBirdDetected => output_root.join(RoutingTier::DIR_NO_SUBJECT_DETECTED),
    }
}

/// Matches a species or subject slug against known multi-taxa categories.
#[must_use]
pub fn lookup_curated_category(slug: &str) -> Option<AssetCategory> {
    let cat = AssetCategory::from_slug(slug);
    if cat == AssetCategory::Bird {
        None
    } else {
        Some(cat)
    }
}

/// Resolves the taxonomic or photographic category from a bundle decision.
///
/// # Arguments
/// * `decision` - Classification decision for an asset bundle.
///
/// # Returns
/// The resolved [`AssetCategory`], returning [`AssetCategory::NoSubjectDetected`]
/// if the subject cannot be identified or classified.
#[must_use]
pub fn resolve_category_for_decision(decision: &BundleDecision) -> AssetCategory {
    if decision.routing_tier == crate::types::RoutingTier::NoBirdDetected {
        return AssetCategory::NoSubjectDetected;
    }

    let from_slug = AssetCategory::from_slug(&decision.assigned_species_slug);
    if from_slug != AssetCategory::NoSubjectDetected {
        return from_slug;
    }

    if let Some(top) = decision.top_predictions.first() {
        let from_pred = top.category();
        if from_pred != AssetCategory::NoSubjectDetected {
            return from_pred;
        }
        let from_taxon_id = AssetCategory::from_slug(&top.taxon_id);
        if from_taxon_id != AssetCategory::NoSubjectDetected {
            return from_taxon_id;
        }
        let from_common = AssetCategory::from_slug(&top.common_name);
        if from_common != AssetCategory::NoSubjectDetected {
            return from_common;
        }
    }

    AssetCategory::NoSubjectDetected
}

/// Computes the target destination directory inside output_root based on routing tier and category.
#[must_use]
pub fn compute_target_dir(output_root: &Path, decision: &BundleDecision) -> PathBuf {
    let category = resolve_category_for_decision(decision);
    compute_target_dir_with_category(output_root, category.as_str(), decision)
}

/// Computes the target destination directory inside output_root given an explicit category and decision.
#[must_use]
pub fn compute_target_dir_with_category(
    output_root: &Path,
    category: &str,
    decision: &BundleDecision,
) -> PathBuf {
    build_output_destination_path(
        output_root,
        category,
        &decision.assigned_species_slug,
        decision.routing_tier,
    )
}

/// Materializes POSIX hard links for an AssetBundle and its companion and sidecar files.
/// Resolves collisions deterministically and handles re-run idempotency.
///
/// # Arguments
/// * `bundle` - Source asset bundle.
/// * `decision` - Classification decision.
/// * `output_root` - Root hierarchy directory.
/// * `run_id` - Current run identifier.
/// * `dry_run` - If true, plans hardlinks without modifying disk.
///
/// # Returns
/// A vector of [`HardlinkOperation`] records.
///
/// # Errors
/// Returns an error if filesystem directory creation or linking fails.
pub fn materialize_bundle_hardlinks(
    bundle: &AssetBundle,
    decision: &BundleDecision,
    output_root: &Path,
    run_id: &str,
    dry_run: bool,
) -> Result<Vec<HardlinkOperation>> {
    let category = resolve_category_for_decision(decision);
    materialize_bundle_hardlinks_with_category(
        bundle,
        decision,
        category.as_str(),
        output_root,
        run_id,
        dry_run,
    )
}

/// Materializes POSIX hard links for an AssetBundle with an explicit taxonomic category.
///
/// # Arguments
/// * `bundle` - Source asset bundle.
/// * `decision` - Classification decision.
/// * `category` - Singular taxonomic or genre category.
/// * `output_root` - Root hierarchy directory.
/// * `run_id` - Current run identifier.
/// * `dry_run` - If true, plans hardlinks without modifying disk.
///
/// # Returns
/// A vector of [`HardlinkOperation`] records.
///
/// # Errors
/// Returns an error if filesystem directory creation or linking fails.
pub fn materialize_bundle_hardlinks_with_category(
    bundle: &AssetBundle,
    decision: &BundleDecision,
    category: &str,
    output_root: &Path,
    run_id: &str,
    dry_run: bool,
) -> Result<Vec<HardlinkOperation>> {
    let target_dir = compute_target_dir_with_category(output_root, category, decision);

    if !dry_run {
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)
                .with_context(|| format!("Failed to create directory: {}", target_dir.display()))?;
        }
        // Backward compatibility: symlink _No_Bird_Detected -> _No_Subject_Detected
        if decision.routing_tier == RoutingTier::NoBirdDetected {
            let legacy_symlink = output_root.join(RoutingTier::DIR_NO_BIRD_DETECTED);
            if !legacy_symlink.exists() && fs::symlink_metadata(&legacy_symlink).is_err() {
                let _ = std::os::unix::fs::symlink(
                    RoutingTier::DIR_NO_SUBJECT_DETECTED,
                    &legacy_symlink,
                );
            }
        }
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
    let primary_filename = bundle.primary_image.file_name().map_or_else(
        || "image.jpg".to_string(),
        |f| f.to_string_lossy().to_string(),
    );

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
            .map_or_else(|| "file".to_string(), |f| f.to_string_lossy().to_string());

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
            .map(std::string::ToString::to_string);

        while dst_path.exists() {
            if let Ok(dst_meta) = fs::metadata(&dst_path) {
                if dst_meta.ino() == src_ino {
                    status = "already_linked".to_string();
                    break;
                }
                // Different file with same name: append clean numbered counter from base
                let new_name = match &base_ext {
                    Some(e) => format!("{base_stem}_{counter}.{e}"),
                    None => format!("{base_stem}_{counter}"),
                };
                dst_path = target_dir.join(new_name);
                counter += 1;
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
                                if let Some(grandparent) = parent.parent() {
                                    affected_dirs.push(grandparent.to_path_buf());
                                }
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
        if let Some(parent) = d.parent() {
            let legacy_symlink = parent.join(RoutingTier::DIR_NO_BIRD_DETECTED);
            if fs::symlink_metadata(&legacy_symlink).is_ok() && !legacy_symlink.exists() {
                let _ = fs::remove_file(&legacy_symlink);
            }
        }
    }

    Ok(cleaned_count)
}

/// Unlinks all generated hard links in output directory for a given run ID,
/// leaving input files completely untouched. Also safely rolls back any in-place renames.
pub fn clean_run_hardlinks(db: &ManifestDb, run_id: &str) -> Result<usize> {
    clean_run_operations(db, run_id)
}

/// Outcome of materializing a chunk of bundles with retry resilience.
#[derive(Debug, Clone)]
pub struct ChunkMaterializationOutcome {
    pub successful_bundles: Vec<AssetBundle>,
    pub failed_bundle_ids: Vec<String>,
    pub was_dataless_flags: Vec<bool>,
    pub space_saved_bytes: u64,
}

/// Materializes a slice of bundles with retry and backoff on transient errors.
///
/// On transient errors, retries once after `retry_backoff`.
/// If the second attempt fails, records the bundle as `pending_download_failed`
/// in SQLite and excludes it from `successful_bundles`.
pub fn materialize_chunk_with_retry(
    bundles: &[AssetBundle],
    backend: &dyn StorageBackend,
    db: &ManifestDb,
    run_id: &str,
    retry_backoff: std::time::Duration,
) -> ChunkMaterializationOutcome {
    let mut successful_bundles = Vec::new();
    let mut failed_bundle_ids = Vec::new();
    let mut was_dataless_flags = Vec::new();
    let mut space_saved_bytes = 0u64;

    for bundle in bundles {
        let loc = backend.inspect_locality(&bundle.primary_image).unwrap_or(
            crate::storage::FileLocality {
                is_dataless: false,
                pre_existing_local: true,
                file_size_bytes: 0,
            },
        );

        let mut mat_res = backend.materialize(&bundle.primary_image);
        if mat_res.is_err() {
            // Retry once after backoff
            if !retry_backoff.is_zero() {
                std::thread::sleep(retry_backoff);
            }
            mat_res = backend.materialize(&bundle.primary_image);
        }

        match mat_res {
            Ok(_) => {
                if loc.is_dataless {
                    space_saved_bytes += loc.file_size_bytes;
                }
                successful_bundles.push(bundle.clone());
                was_dataless_flags.push(loc.is_dataless);
            }
            Err(e) => {
                let err_msg = format!("{e}");
                eprintln!(
                    "Warning: Failed to materialize asset '{}': {}. Marking as pending retry.",
                    bundle.primary_image.display(),
                    err_msg
                );
                let _ = db.record_pending_download_failure(run_id, bundle, &err_msg);
                failed_bundle_ids.push(bundle.bundle_id.clone());
            }
        }
    }

    ChunkMaterializationOutcome {
        successful_bundles,
        failed_bundle_ids,
        was_dataless_flags,
        space_saved_bytes,
    }
}

/// Reverts all in-place renames for a given run ID.
pub fn clean_run_inplace(db: &ManifestDb, run_id: &str) -> Result<usize> {
    db.rollback_inplace_renames(run_id)
}

/// Determines whether an existing cached photo bundle should be re-evaluated and reclassified.
///
/// # Invariants & Reclassification Precedence
/// 1. **User Overrides are Permanent & Inviolable**:
///    If `existing_decision.is_user_override` is `true`, this function returns `false` under all
///    circumstances. Explicit human corrections recorded via `avicull correct` or the HTML catalog
///    must never be clobbered by automatic reclassification or flag sweeps.
/// 2. **Explicit `--reclassify` Opt-In**:
///    If `force_reclassify` is `true`, all non-override cached bundles are queued for re-classification.
/// 3. **Spatial & Coordinate Prior Updates**:
///    If `bundle.gps_coords != old_bundle.gps_coords`, the bundle's geographic location has changed
///    (e.g. from newly assigned EXIF, parent directory gazetteer resolution, or catalog override),
///    requiring Bayesian spatial-temporal prior re-evaluation (`true`).
/// 4. **Idempotent Retention**:
///    Bundles with existing valid classifications and identical coordinates return `false`.
///
/// # Arguments
/// * `bundle` - The currently discovered asset bundle with up-to-date resolved coordinates.
/// * `existing_decision` - The previously recorded decision from the SQLite manifest.
/// * `old_bundle` - The cached asset bundle metadata from the SQLite manifest.
/// * `force_reclassify` - Whether the user supplied `--reclassify`.
///
/// # Returns
/// `true` if the bundle requires re-classification; `false` if the cached decision should be reused.
#[must_use]
pub fn should_reclassify_bundle(
    bundle: &AssetBundle,
    existing_decision: &BundleDecision,
    old_bundle: &AssetBundle,
    force_reclassify: bool,
) -> bool {
    if existing_decision.is_user_override {
        return false;
    }
    if force_reclassify {
        return true;
    }
    if bundle.gps_coords != old_bundle.gps_coords {
        return true;
    }
    false
}

/// Regenerates `catalog.html` directly from the SQLite manifest database.
///
/// # Arguments
/// * `db` - Open `ManifestDb` instance containing bundle decisions and records.
/// * `operating_root` - Root directory where `catalog.html` will be placed.
/// * `trip_name` - Optional display name for the trip header in the catalog.
/// * `mode` - Organization mode; if `None`, looks up the latest run mode from `db`.
///
/// # Returns
/// The destination path of the generated HTML catalog.
///
/// # Errors
/// Returns `Err` if SQLite read operations or catalog HTML emission fail.
pub fn regenerate_catalog_from_manifest(
    db: &ManifestDb,
    operating_root: &Path,
    trip_name: Option<&str>,
    mode: Option<OrganizationMode>,
) -> Result<PathBuf> {
    let catalog_path = operating_root.join(crate::catalog::DEFAULT_CATALOG_FILENAME);
    let effective_mode = match mode {
        Some(m) => m,
        None => db
            .get_latest_run_mode()?
            .unwrap_or(OrganizationMode::InPlace),
    };

    let canonical_root = operating_root.canonicalize().ok();
    let fallback_trip_name = canonical_root
        .as_deref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .or_else(|| operating_root.file_name().and_then(|n| n.to_str()))
        .unwrap_or("Avian Photos");
    let effective_trip_name = trip_name.unwrap_or(fallback_trip_name);

    let all_db_bundles = db.get_all_bundles(None)?;
    let bundle_locs = db.get_all_bundle_locations().unwrap_or_default();
    let dir_locs = db.get_all_directory_locations().unwrap_or_default();
    let mut catalog_items = Vec::with_capacity(all_db_bundles.len());

    for (dec, mut b) in all_db_bundles {
        crate::location::resolver::apply_location_precedence_cached(
            &mut b,
            &bundle_locs,
            &dir_locs,
        );
        let (prim_dest, comp_dests, sidecar_dests) = match effective_mode {
            OrganizationMode::HierarchicalHardlinkHierarchy => {
                let ops = db.get_bundle_operations(&b.bundle_id).unwrap_or_default();
                let p_dest = ops
                    .iter()
                    .find(|op| op.source_path == b.primary_image && op.operation_type == "hardlink")
                    .map(|op| op.dest_path.clone())
                    .or_else(|| {
                        let expected = compute_target_dir(operating_root, &dec);
                        let dst_name = b.primary_image.file_name()?;
                        let p = expected.join(dst_name);
                        if p.exists() {
                            Some(p)
                        } else {
                            None
                        }
                    });
                let c_dests: Vec<PathBuf> = b
                    .companion_images
                    .iter()
                    .filter_map(|c| {
                        ops.iter()
                            .find(|op| op.source_path == *c && op.operation_type == "hardlink")
                            .map(|op| op.dest_path.clone())
                    })
                    .collect();
                let sc_dests: Vec<PathBuf> = b
                    .sidecar_files
                    .iter()
                    .filter_map(|sc| {
                        ops.iter()
                            .find(|op| op.source_path == *sc && op.operation_type == "hardlink")
                            .map(|op| op.dest_path.clone())
                    })
                    .collect();
                (p_dest, Some(c_dests), Some(sc_dests))
            }
            OrganizationMode::InPlace => (
                Some(b.primary_image.clone()),
                Some(b.companion_images.clone()),
                Some(b.sidecar_files.clone()),
            ),
            OrganizationMode::InPlaceRename => {
                let ops = db.get_bundle_operations(&b.bundle_id).unwrap_or_default();
                let mut current_primary = b.primary_image.clone();
                for op in &ops {
                    if op.operation_type == "in_place_rename"
                        && op.status == "renamed"
                        && op.dest_path.exists()
                    {
                        current_primary.clone_from(&op.dest_path);
                    }
                }

                let c_dests: Vec<PathBuf> = b
                    .companion_images
                    .iter()
                    .map(|c| {
                        let mut resolved = c.clone();
                        for op in &ops {
                            if op.operation_type == "in_place_rename"
                                && op.status == "renamed"
                                && (op.source_path == *c || op.source_path == resolved)
                                && op.dest_path.exists()
                            {
                                resolved.clone_from(&op.dest_path);
                            }
                        }
                        resolved
                    })
                    .collect();

                let sc_dests: Vec<PathBuf> = b
                    .sidecar_files
                    .iter()
                    .map(|sc| {
                        let mut resolved = sc.clone();
                        for op in &ops {
                            if op.operation_type == "in_place_rename"
                                && op.status == "renamed"
                                && (op.source_path == *sc || op.source_path == resolved)
                                && op.dest_path.exists()
                            {
                                resolved.clone_from(&op.dest_path);
                            }
                        }
                        resolved
                    })
                    .collect();

                (Some(current_primary), Some(c_dests), Some(sc_dests))
            }
        };

        catalog_items.push(CatalogItem::from_bundle_and_decision(
            &b,
            &dec,
            prim_dest,
            comp_dests,
            sidecar_dests,
        ));
    }

    crate::catalog::generate_html_catalog(
        effective_trip_name,
        &catalog_items,
        effective_mode,
        &catalog_path,
    )?;

    Ok(catalog_path)
}
