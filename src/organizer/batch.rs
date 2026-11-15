//! # Batch Subdirectory Organization (`organize-all`)
//!
//! Orchestrates sequential organization of multi-trip photo directories:
//! - Discovers immediate subdirectories under `--input`.
//! - Filters out hidden directories (`.*`), `birds_hierarchy`, `@eaDir`, and `.photoslibrary` bundles.
//! - Sorts candidate subdirectories alphabetically for deterministic execution.
//! - Executes [`run_organize`] sequentially on each child directory, creating independent child
//!   manifests (`avicull_catalog_files/manifest.sqlite`) and standalone catalogs (`catalog.html`).
//! - If `no_parent_catalog` is false, runs a final root pass to discover and roll up all child
//!   manifests into an aggregated root catalog and manifest.

use crate::catalog::{
    generate_html_catalog_with_policy, resolve_manifest_db_path, CatalogItem,
    ThumbnailEmbeddingPolicy, CATALOG_FILES_DIR, DEFAULT_CATALOG_FILENAME,
};
use crate::cli::{OrganizeAllArgs, OrganizeArgs};
use crate::db::ManifestDb;
use crate::organizer::run_organize;
use crate::types::OrganizationMode;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Filters entry names that must be skipped during subdirectory scanning.
///
/// Skips:
/// - Hidden directories (names starting with `.`)
/// - `birds_hierarchy` (avoids infinite loops or scanning previous hierarchy output)
/// - `@eaDir` (Synology NAS indexing metadata)
/// - Directories ending with `.photoslibrary` (Apple Photos packages)
/// - `avicull_catalog_files` (unhidden catalog assets directory)
#[must_use]
pub fn should_skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || name == "birds_hierarchy"
        || name == "@eaDir"
        || name.ends_with(".photoslibrary")
        || name == crate::catalog::CATALOG_FILES_DIR
}

/// Discovers immediate candidate subdirectories under `input_dir` sorted alphabetically.
///
/// # Arguments
/// * `input_dir` - Path to the root directory containing trip subdirectories.
///
/// # Returns
/// Sorted list of immediate child directory [`PathBuf`]s to organize.
///
/// # Errors
/// Returns an error if directory traversal or metadata reads fail.
pub fn discover_child_directories(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let entries = fs::read_dir(input_dir)
        .with_context(|| format!("Failed to read input directory '{}'", input_dir.display()))?;

    for entry_res in entries {
        let entry = entry_res.with_context(|| {
            format!(
                "Failed to read directory entry in '{}'",
                input_dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if !should_skip_directory(file_name) {
                    dirs.push(path);
                }
            }
        }
    }

    dirs.sort();
    Ok(dirs)
}

/// Constructs child [`OrganizeArgs`] for an immediate subdirectory.
///
/// Forwards options from the parent batch invocation while targeting `child_dir` as input.
///
/// # Arguments
/// * `child_dir` - Path to the child subdirectory.
/// * `args` - Batch runner configuration arguments.
///
/// # Returns
/// An [`OrganizeArgs`] instance customized for the child directory.
#[must_use]
pub fn build_child_organize_args(child_dir: &Path, args: &OrganizeAllArgs) -> OrganizeArgs {
    let child_output = args
        .output
        .as_ref()
        .map(|out_root| out_root.join(child_dir.file_name().unwrap_or_default()));

    OrganizeArgs {
        input: child_dir.to_path_buf(),
        output: child_output,
        dry_run: false,
        force: args.force,
        reclassify: false,
        view_only: args.view_only,
        min_confidence: args.min_confidence,
        min_detect: args.min_detect,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: args.mode,
        geo_prior_weight: 1.0,
        locations_file: args.locations_file.clone(),
        default_lat: args.default_lat,
        default_lon: args.default_lon,
        models_dir: args.models_dir.clone(),
        threads: args.threads,
        location: None,
        no_prompt: args.no_prompt,
        max_cache_size: "5GB".to_string(),
        verbose: args.verbose,
    }
}

/// Constructs parent rollup [`OrganizeArgs`] for the root directory.
///
/// # Arguments
/// * `args` - Batch runner configuration arguments.
///
/// # Returns
/// An [`OrganizeArgs`] instance targeting the root directory for rollup catalog generation.
#[must_use]
pub fn build_parent_organize_args(args: &OrganizeAllArgs) -> OrganizeArgs {
    OrganizeArgs {
        input: args.input.clone(),
        output: args.output.clone(),
        dry_run: false,
        force: args.force,
        reclassify: false,
        view_only: args.view_only,
        min_confidence: args.min_confidence,
        min_detect: args.min_detect,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: args.mode,
        geo_prior_weight: 1.0,
        locations_file: args.locations_file.clone(),
        default_lat: args.default_lat,
        default_lon: args.default_lon,
        models_dir: args.models_dir.clone(),
        threads: args.threads,
        location: None,
        no_prompt: args.no_prompt,
        max_cache_size: "5GB".to_string(),
        verbose: args.verbose,
    }
}

/// Executes sequential organization across all immediate subdirectories in `args.input`.
///
/// Discovers subdirectories, filters ignored folders, sorts them alphabetically,
/// organizes each child folder with independent catalogs and manifests, and optionally
/// rolls up all child manifests into a unified root catalog.
///
/// Fault-tolerant: if an individual child folder encounters an error, logs a warning
/// and proceeds with the remaining folders. If all child folders fail, returns an error.
///
/// # Arguments
/// * `args` - Configuration parameters defining input path, mode, and ML options.
///
/// # Returns
/// `Ok(())` upon successful execution.
///
/// # Errors
/// Returns error if directory traversal fails or all child directories fail to organize.
pub fn run_organize_all(args: &OrganizeAllArgs) -> Result<()> {
    println!("== avicull organize-all ==");
    if let Some(t) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build_global()
            .ok();
    }

    println!(
        "Scanning '{}' for immediate subdirectories...",
        args.input.display()
    );

    let child_dirs = discover_child_directories(&args.input)?;
    println!(
        "Found {} subdirectories to organize sequentially.",
        child_dirs.len()
    );

    let total_count = child_dirs.len();
    let mut successful_count = 0;
    let mut failed_subdirs: Vec<(PathBuf, anyhow::Error)> = Vec::new();

    for (idx, child_dir) in child_dirs.iter().enumerate() {
        println!(
            "\n[{}/{}] Organizing subdirectory '{}'...",
            idx + 1,
            total_count,
            child_dir.display()
        );

        if !args.force && !args.view_only {
            match crate::scanner::probe::probe_directory_change_status(child_dir) {
                Ok(crate::scanner::probe::DirectoryProbeResult::UpToDate) => {
                    println!(
                        "  ⚡ Subdirectory '{}' is already up to date (unchanged signature). Skipping.",
                        child_dir.display()
                    );
                    successful_count += 1;
                    continue;
                }
                Ok(crate::scanner::probe::DirectoryProbeResult::RequiresProcessing(ref reason)) => {
                    if args.verbose {
                        println!(
                            "  Subdirectory '{}' requires processing: {reason}",
                            child_dir.display()
                        );
                    }
                }
                Err(e) => {
                    if args.verbose {
                        eprintln!(
                            "  Warning: probe failed for '{}': {e}. Proceeding with organize.",
                            child_dir.display()
                        );
                    }
                }
            }
        }

        let child_args = build_child_organize_args(child_dir, args);
        match run_organize(&child_args) {
            Ok(()) => {
                successful_count += 1;
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to organize child subdirectory '{}': {e}",
                    child_dir.display()
                );
                failed_subdirs.push((child_dir.clone(), e));
            }
        }
    }

    println!("\nOrganized {successful_count}/{total_count} subdirectories successfully.");

    if !failed_subdirs.is_empty() {
        eprintln!("\nFailed subdirectories:");
        for (failed_dir, err) in &failed_subdirs {
            eprintln!("  - {}: {err}", failed_dir.display());
        }
    }

    if total_count > 0 && successful_count == 0 {
        bail!("All {total_count} child subdirectories failed to organize.");
    }

    if !args.no_parent_catalog && successful_count > 0 {
        println!(
            "\n[Rollup] Generating aggregated parent catalog for '{}'...",
            args.input.display()
        );
        run_parent_rollup(args, &child_dirs).with_context(|| {
            format!(
                "Failed to generate parent rollup catalog for '{}'",
                args.input.display()
            )
        })?;
    }

    println!("\n✓ organize-all completed successfully.");
    Ok(())
}

/// Executes aggregated parent rollup by synchronizing child manifests into a staged SQLite database
/// and generating an aggregated parent catalog with zero image I/O.
///
/// # Invariants Guaranteed
/// 1. **Zero-Image I/O**: Performs 0 reads or decodes against RAW or companion image assets.
/// 2. **Network Lock Avoidance**: Uses a local staging database in `/tmp/` before atomically
///    committing to `<parent>/avicull_catalog_files/manifest.sqlite`.
/// 3. **Dual Thumbnail Optimization**: Suppresses base64 data URIs in the root rollup catalog,
///    linking directly to child preview files (`{child}/avicull_catalog_files/previews/{stem}.jpg`).
/// 4. **Pruning Synchronization**: Automatically prunes child records from parent state if their
///    directory is no longer present on disk.
///
/// # Arguments
/// * `args` - Batch runner configuration arguments.
/// * `child_dirs` - Slice of candidate child directories to roll up.
///
/// # Returns
/// `Ok(())` upon successful rollup.
///
/// # Errors
/// Returns an error if filesystem operations or database queries fail.
pub fn run_parent_rollup(args: &OrganizeAllArgs, child_dirs: &[PathBuf]) -> Result<()> {
    let operating_root = if args.mode == OrganizationMode::HierarchicalHardlinkHierarchy {
        args.output
            .clone()
            .unwrap_or_else(|| args.input.join("birds_hierarchy"))
    } else {
        args.input.clone()
    };

    let parent_manifest_path = operating_root
        .join(CATALOG_FILES_DIR)
        .join("manifest.sqlite");
    let catalog_path = operating_root.join(DEFAULT_CATALOG_FILENAME);
    let trip_name = args
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Avian Photos Archive");

    // Use local NVMe / temporary storage for SQLite staging to avoid SMB/NFS lock contention
    let temp_dir = tempfile::tempdir()
        .context("Failed to create temporary directory for parent rollup staging")?;
    let staging_db_path = temp_dir.path().join("staging_manifest.sqlite");

    if parent_manifest_path.exists() {
        if let Ok(parent_db) = ManifestDb::open(&parent_manifest_path) {
            let _ = parent_db.checkpoint();
        }
        fs::copy(&parent_manifest_path, &staging_db_path).with_context(|| {
            format!(
                "Failed to copy existing parent manifest '{}' to staging",
                parent_manifest_path.display()
            )
        })?;
    }

    let staging_db = ManifestDb::open_or_create(&staging_db_path)?;

    let mut valid_child_rel_paths = Vec::with_capacity(child_dirs.len());
    for child_dir in child_dirs {
        let child_name = child_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if child_name.is_empty() {
            continue;
        }
        valid_child_rel_paths.push(child_name.clone());

        let child_output = args.output.as_ref().map(|out| out.join(&child_name));
        let child_manifest = resolve_manifest_db_path(
            None,
            child_output.as_deref(),
            Some(child_dir),
            Some(args.mode),
        );

        if !child_manifest.exists() {
            if args.verbose {
                println!(
                    "  Child '{}' manifest not found at '{}', skipping rollup for this child.",
                    child_name,
                    child_manifest.display()
                );
            }
            continue;
        }

        let child_db = match ManifestDb::open(&child_manifest) {
            Ok(db) => db,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to open child manifest '{}': {e}",
                    child_manifest.display()
                );
                continue;
            }
        };

        let latest_run_id = child_db.get_latest_run_id()?.unwrap_or_default();

        let is_synced = if args.force {
            false
        } else if let Ok(Some(sync)) = staging_db.get_child_sync(&child_name) {
            !latest_run_id.is_empty() && sync.child_last_run_id == latest_run_id
        } else {
            false
        };

        if is_synced {
            if args.verbose {
                println!(
                    "  ⚡ Child '{child_name}' manifest is already synced with run {latest_run_id}. Keeping existing records."
                );
            }
        } else {
            if args.verbose {
                println!(
                    "  Importing child '{child_name}' manifest into parent staging database..."
                );
            }
            staging_db.import_child_manifest(&child_manifest, Path::new(&child_name))?;
        }
    }

    // Prune child records whose directory no longer exists on disk
    let pruned = staging_db.prune_missing_children(&valid_child_rel_paths)?;
    if pruned > 0 && args.verbose {
        println!("  Pruned {pruned} missing child directories from parent manifest.");
    }

    // Assemble catalog items from staging DB (0 image reads)
    let all_bundles = staging_db.get_all_bundles(None)?;
    let mut catalog_items = Vec::with_capacity(all_bundles.len());
    for (decision, bundle) in &all_bundles {
        let mut item = CatalogItem::from_bundle_and_decision(bundle, decision, None, None, None);
        let child_sub = bundle
            .relative_path
            .as_ref()
            .and_then(|p| p.iter().next().map(|c| c.to_string_lossy().to_string()))
            .unwrap_or_else(|| bundle.trip_folder_name.clone());
        item.subdirectory = Some(child_sub);
        catalog_items.push(item);
    }

    // Flush staging DB checkpoint and drop connection before file copy
    let _ = staging_db.checkpoint();
    drop(staging_db);

    // Atomically commit staged database to target parent manifest path
    if let Some(parent) = parent_manifest_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for parent manifest at '{}'",
                parent.display()
            )
        })?;
    }
    let tmp_parent_manifest = parent_manifest_path.with_extension("tmp");
    fs::copy(&staging_db_path, &tmp_parent_manifest).with_context(|| {
        format!(
            "Failed to stage parent manifest copy at '{}'",
            tmp_parent_manifest.display()
        )
    })?;
    fs::rename(&tmp_parent_manifest, &parent_manifest_path).with_context(|| {
        format!(
            "Failed to atomically commit parent manifest to '{}'",
            parent_manifest_path.display()
        )
    })?;
    let _ = fs::remove_file(parent_manifest_path.with_extension("sqlite-wal"));
    let _ = fs::remove_file(parent_manifest_path.with_extension("sqlite-shm"));

    // Update parent directory sync state
    if let Ok(sig) = crate::scanner::probe::compute_directory_signature(&args.input) {
        if let Ok(parent_db) = ManifestDb::open(&parent_manifest_path) {
            let _ = parent_db.set_directory_sync_state(".", &sig);
        }
    }

    // Generate root catalog.html with ThumbnailEmbeddingPolicy::Suppressed
    println!(
        "Generating aggregated parent offline HTML photo catalog at '{}'...",
        catalog_path.display()
    );
    generate_html_catalog_with_policy(
        trip_name,
        &catalog_items,
        args.mode,
        &catalog_path,
        ThumbnailEmbeddingPolicy::Suppressed,
    )?;

    println!(
        "✓ Aggregated parent rollup complete ({} photos across {} trips).",
        catalog_items.len(),
        child_dirs.len()
    );

    Ok(())
}
