//! # Batch Subdirectory Organization (`organize-all`)
//!
//! Orchestrates sequential organization of multi-trip photo directories:
//! - Discovers immediate subdirectories under `--input`.
//! - Filters out hidden directories (`.*`), `birds_hierarchy`, `@eaDir`, and `.photoslibrary` bundles.
//! - Sorts candidate subdirectories alphabetically for deterministic execution.
//! - Executes [`run_organize`] sequentially on each child directory, creating independent child
//!   manifests (`.avicull/manifest.sqlite`) and standalone catalogs (`catalog.html`).
//! - If `no_parent_catalog` is false, runs a final root pass to discover and roll up all child
//!   manifests into an aggregated root catalog and manifest.

use crate::cli::{OrganizeAllArgs, OrganizeArgs};
use crate::organizer::run_organize;
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
#[must_use]
pub fn should_skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || name == "birds_hierarchy"
        || name == "@eaDir"
        || name.ends_with(".photoslibrary")
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
        let parent_args = build_parent_organize_args(args);
        run_organize(&parent_args).with_context(|| {
            format!(
                "Failed to generate parent rollup catalog for '{}'",
                args.input.display()
            )
        })?;
    }

    println!("\n✓ organize-all completed successfully.");
    Ok(())
}
