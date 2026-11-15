//! # Tests for Directory-Only Walk & Composite Signature Change Detection
//!
//! Validates:
//! - Fresh directory without manifest returns `RequiresProcessing(ManifestMissing)`.
//! - After organizing a directory, `probe_directory_change_status` returns `UpToDate`.
//! - Adding a file into a deep nested directory alters signature and returns `RequiresProcessing`.
//! - Adding a photo to the top-level directory alters `total_entry_count` and returns `RequiresProcessing`.
//! - Removing `catalog.html` returns `RequiresProcessing(CatalogMissing)`.
//! - `compute_directory_signature` strictly excludes skipped folders (`avicull_catalog_files`, legacy `.avicull`, `birds_hierarchy`, `@eaDir`, hidden dirs).
//! - `organize-all` skips unchanged child directories on second invocation.

use avicull::catalog::CATALOG_FILES_DIR;
use avicull::cli::{OrganizeAllArgs, OrganizeArgs};
use avicull::organizer::{run_organize, run_organize_all};
use avicull::scanner::probe::{
    compute_directory_signature, probe_directory_change_status, ChangeReason, DirectoryProbeResult,
};
use avicull::types::OrganizationMode;
use std::fs;
use tempfile::tempdir;

fn build_test_organize_args(input: &std::path::Path) -> OrganizeArgs {
    OrganizeArgs {
        input: input.to_path_buf(),
        output: None,
        dry_run: false,
        force: false,
        reclassify: false,
        reclassify_mode: avicull::cli::ReclassifyMode::Fast,
        view_only: false,
        min_confidence: 0.65,
        min_detect: 0.25,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: OrganizationMode::InPlace,
        geo_prior_weight: 1.0,
        locations_file: None,
        default_lat: None,
        default_lon: None,
        models_dir: None,
        threads: Some(1),
        location: None,
        no_prompt: true,
        max_cache_size: "5GB".to_string(),
        verbose: false,
    }
}

#[test]
fn test_probe_fresh_directory_requires_processing_manifest_missing() {
    let temp = tempdir().expect("create tempdir");
    let fresh_dir = temp.path().join("FreshTrip");
    fs::create_dir_all(&fresh_dir).expect("create fresh_dir");
    fs::write(fresh_dir.join("photo1.jpg"), b"fake_jpeg_bytes").expect("write photo");

    let probe_res = probe_directory_change_status(&fresh_dir).expect("probe status");
    assert_eq!(
        probe_res,
        DirectoryProbeResult::RequiresProcessing(ChangeReason::ManifestMissing),
        "Fresh directory lacking manifest must report ManifestMissing"
    );
    assert!(!probe_res.is_up_to_date());
}

#[test]
fn test_probe_directory_up_to_date_after_organize() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip2024");
    fs::create_dir_all(&trip_dir).expect("create trip_dir");
    fs::write(trip_dir.join("photo1.jpg"), b"fake_jpeg_bytes").expect("write photo");

    let args = build_test_organize_args(&trip_dir);
    run_organize(&args).expect("run organize");

    assert!(
        trip_dir
            .join(CATALOG_FILES_DIR)
            .join("manifest.sqlite")
            .exists(),
        "Manifest must exist after organize"
    );
    assert!(
        trip_dir.join("catalog.html").exists(),
        "Catalog HTML must exist after organize"
    );

    let probe_res = probe_directory_change_status(&trip_dir).expect("probe status");
    assert_eq!(
        probe_res,
        DirectoryProbeResult::UpToDate,
        "Clean directory post-organization must probe as UpToDate"
    );
    assert!(probe_res.is_up_to_date());
}

#[test]
fn test_probe_detects_nested_directory_addition() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("NestedTrip");
    let nested_cam = trip_dir.join("Day1").join("CameraA");
    fs::create_dir_all(&nested_cam).expect("create nested directory");
    fs::write(nested_cam.join("IMG_0001.JPG"), b"fake_jpeg_1").expect("write img1");

    let args = build_test_organize_args(&trip_dir);
    run_organize(&args).expect("run organize");

    let initial_probe = probe_directory_change_status(&trip_dir).expect("initial probe");
    assert_eq!(
        initial_probe,
        DirectoryProbeResult::UpToDate,
        "Initial status should be UpToDate"
    );

    // Add a new file inside the deep nested directory
    fs::write(nested_cam.join("IMG_0002.CR2"), b"fake_raw_2").expect("write img2");

    let post_add_probe = probe_directory_change_status(&trip_dir).expect("post add probe");
    match post_add_probe {
        DirectoryProbeResult::RequiresProcessing(ChangeReason::SignatureMismatch {
            expected,
            actual,
        }) => {
            assert_eq!(
                actual.total_entry_count,
                expected.total_entry_count + 1,
                "Actual entry count should reflect the newly added nested file"
            );
        }
        other => panic!("Expected SignatureMismatch, got: {other:?}"),
    }
}

#[test]
fn test_probe_detects_top_level_directory_addition() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("TopLevelTrip");
    fs::create_dir_all(&trip_dir).expect("create trip_dir");
    fs::write(trip_dir.join("IMG_0001.JPG"), b"fake_jpeg_1").expect("write img1");

    let args = build_test_organize_args(&trip_dir);
    run_organize(&args).expect("run organize");

    let initial_probe = probe_directory_change_status(&trip_dir).expect("initial probe");
    assert_eq!(initial_probe, DirectoryProbeResult::UpToDate);

    // Add another photo directly in trip root
    fs::write(trip_dir.join("IMG_0002.JPG"), b"fake_jpeg_2").expect("write img2");

    let post_add_probe = probe_directory_change_status(&trip_dir).expect("post add probe");
    match post_add_probe {
        DirectoryProbeResult::RequiresProcessing(ChangeReason::SignatureMismatch {
            expected,
            actual,
        }) => {
            assert_eq!(
                actual.total_entry_count,
                expected.total_entry_count + 1,
                "Top-level addition must increment total_entry_count"
            );
        }
        other => panic!("Expected SignatureMismatch, got: {other:?}"),
    }
}

#[test]
fn test_probe_detects_missing_catalog_html() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("MissingCatalogTrip");
    fs::create_dir_all(&trip_dir).expect("create trip_dir");
    fs::write(trip_dir.join("photo1.jpg"), b"fake_jpeg_bytes").expect("write photo");

    let args = build_test_organize_args(&trip_dir);
    run_organize(&args).expect("run organize");

    assert_eq!(
        probe_directory_change_status(&trip_dir).expect("probe status"),
        DirectoryProbeResult::UpToDate
    );

    // Remove catalog.html
    let catalog_file = trip_dir.join("catalog.html");
    fs::remove_file(&catalog_file).expect("remove catalog.html");

    let probe_res = probe_directory_change_status(&trip_dir).expect("probe status");
    assert_eq!(
        probe_res,
        DirectoryProbeResult::RequiresProcessing(ChangeReason::CatalogMissing),
        "Removing catalog.html must trigger CatalogMissing"
    );
}

#[test]
fn test_compute_directory_signature_skips_excluded_directories() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("ExcludedDirsTrip");
    fs::create_dir_all(&trip_dir).expect("create trip_dir");

    // Legitimate photo
    fs::write(trip_dir.join("IMG_0001.JPG"), b"photo").expect("write photo");

    // Catalog files folder
    let cat_files = trip_dir.join(CATALOG_FILES_DIR).join("previews");
    fs::create_dir_all(&cat_files).expect("create cat_files");
    fs::write(cat_files.join("IMG_0001.jpg"), b"thumb").expect("write thumb");

    // Legacy catalog folder
    let legacy_cat = trip_dir.join(".avicull").join("previews");
    fs::create_dir_all(&legacy_cat).expect("create legacy_cat");
    fs::write(legacy_cat.join("IMG_0001.jpg"), b"legacy thumb").expect("write legacy thumb");

    // Hierarchy folder
    let hier = trip_dir.join("birds_hierarchy");
    fs::create_dir_all(&hier).expect("create hier");
    fs::write(hier.join("dummy.txt"), b"hier").expect("write hier file");

    // NAS indexing folder
    let eadir = trip_dir.join("@eaDir");
    fs::create_dir_all(&eadir).expect("create eadir");
    fs::write(eadir.join("index"), b"eadir").expect("write eadir");

    // Hidden folder
    let hidden = trip_dir.join(".hidden");
    fs::create_dir_all(&hidden).expect("create hidden");
    fs::write(hidden.join("secret.txt"), b"hidden").expect("write hidden");

    // Catalog html
    fs::write(trip_dir.join("catalog.html"), b"<html></html>").expect("write catalog");

    let sig = compute_directory_signature(&trip_dir).expect("compute sig");

    assert_eq!(
        sig.total_entry_count, 1,
        "Only the legitimate photo must be counted, all internal and hidden assets skipped"
    );
    assert_eq!(
        sig.dir_node_count, 1,
        "Only the root directory node itself must be counted"
    );
}

#[test]
fn test_organize_all_skips_unchanged_directories() {
    let temp = tempdir().expect("create tempdir");
    let root = temp.path().join("BatchRoot");
    let trip_a = root.join("TripAlpha");
    let trip_b = root.join("TripBeta");
    fs::create_dir_all(&trip_a).expect("create trip_a");
    fs::create_dir_all(&trip_b).expect("create trip_b");

    fs::write(trip_a.join("photoA.jpg"), b"fake_jpeg_a").expect("write photoA");
    fs::write(trip_b.join("photoB.jpg"), b"fake_jpeg_b").expect("write photoB");

    let all_args = OrganizeAllArgs {
        input: root.clone(),
        output: None,
        no_parent_catalog: false,
        mode: OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        reclassify_mode: avicull::cli::ReclassifyMode::Fast,
        view_only: false,
        verbose: true,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    };

    // First invocation: organizes both child directories and parent rollup
    run_organize_all(&all_args).expect("first run_organize_all");

    assert_eq!(
        probe_directory_change_status(&trip_a).expect("probe trip_a"),
        DirectoryProbeResult::UpToDate,
        "TripAlpha must be up to date"
    );
    assert_eq!(
        probe_directory_change_status(&trip_b).expect("probe trip_b"),
        DirectoryProbeResult::UpToDate,
        "TripBeta must be up to date"
    );

    // Second invocation: both child directories are probed UpToDate and skipped
    run_organize_all(&all_args).expect("second run_organize_all");
}
