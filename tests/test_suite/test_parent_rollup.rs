//! # Integration & Unit Tests for Zero-Image-I/O Parent Rollup & SQLite Staging
//!
//! Validates:
//! - Parent rollup operates with zero image I/O (succeeds even when RAWs are chmod 000).
//! - Incremental sync records child runs and re-uses records without redundant re-import.
//! - Pruning missing/deleted child trips cleans up parent manifest and catalog.

use avicull::catalog::{CATALOG_FILES_DIR, DEFAULT_CATALOG_FILENAME};
use avicull::cli::{OrganizeAllArgs, OrganizeArgs};
use avicull::db::ManifestDb;
use avicull::organizer::{run_organize, run_parent_rollup};
use avicull::types::OrganizationMode;
use std::fs;
use tempfile::tempdir;

fn make_test_organize_args(input: std::path::PathBuf) -> OrganizeArgs {
    OrganizeArgs {
        input,
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
fn test_parent_rollup_zero_image_io() {
    let temp = tempdir().expect("create tempdir");
    let root = temp.path();

    // 1. Create two mock child trips
    let trip_a = root.join("Trip_A");
    let trip_b = root.join("Trip_B");
    fs::create_dir_all(&trip_a).expect("create trip_a");
    fs::create_dir_all(&trip_b).expect("create trip_b");

    let raw_a = trip_a.join("IMG_0001.CR2");
    let raw_b = trip_b.join("IMG_0002.CR2");
    fs::write(&raw_a, b"fake_cr2_payload_a").expect("write raw_a");
    fs::write(&raw_b, b"fake_cr2_payload_b").expect("write raw_b");

    // 2. Organize children individually
    let args_a = make_test_organize_args(trip_a.clone());
    run_organize(&args_a).expect("organize trip_a");

    let args_b = make_test_organize_args(trip_b.clone());
    run_organize(&args_b).expect("organize trip_b");

    assert!(trip_a
        .join(CATALOG_FILES_DIR)
        .join("manifest.sqlite")
        .exists());
    assert!(trip_b
        .join(CATALOG_FILES_DIR)
        .join("manifest.sqlite")
        .exists());

    // 3. Make all RAW image files completely unreadable (chmod 000 on Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&raw_a, fs::Permissions::from_mode(0o000)).expect("chmod 000 raw_a");
        fs::set_permissions(&raw_b, fs::Permissions::from_mode(0o000)).expect("chmod 000 raw_b");
    }

    // 4. Run parent rollup
    let all_args = OrganizeAllArgs {
        input: root.to_path_buf(),
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

    let child_dirs = vec![trip_a.clone(), trip_b.clone()];
    run_parent_rollup(&all_args, &child_dirs)
        .expect("parent rollup must succeed with zero image I/O");

    // 5. Restore permissions for tempdir cleanup on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&raw_a, fs::Permissions::from_mode(0o644));
        let _ = fs::set_permissions(&raw_b, fs::Permissions::from_mode(0o644));
    }

    // 6. Assert parent catalog.html and manifest.sqlite exist
    let parent_manifest = root.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    let parent_catalog = root.join(DEFAULT_CATALOG_FILENAME);
    assert!(parent_manifest.exists(), "Parent manifest must exist");
    assert!(parent_catalog.exists(), "Parent catalog.html must exist");

    // Assert parent catalog HTML contains preview links with child subdirectories
    let html = fs::read_to_string(&parent_catalog).expect("read parent catalog");
    assert!(html.contains("Trip_A/avicull_catalog_files/previews/IMG_0001.jpg"));
    assert!(html.contains("Trip_B/avicull_catalog_files/previews/IMG_0002.jpg"));
}

#[test]
fn test_parent_rollup_incremental_sync() {
    let temp = tempdir().expect("create tempdir");
    let root = temp.path();

    let trip_a = root.join("Trip_A");
    fs::create_dir_all(&trip_a).expect("create trip_a");
    fs::write(trip_a.join("IMG_0001.JPG"), b"fake_jpeg_1").expect("write img1");

    let args_a = make_test_organize_args(trip_a.clone());
    run_organize(&args_a).expect("organize trip_a");

    let all_args = OrganizeAllArgs {
        input: root.to_path_buf(),
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

    let child_dirs = vec![trip_a.clone()];
    run_parent_rollup(&all_args, &child_dirs).expect("first parent rollup");

    let parent_manifest = root.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    let db = ManifestDb::open(&parent_manifest).expect("open parent db");

    let sync_rec = db
        .get_child_sync("Trip_A")
        .expect("get sync record")
        .expect("sync record must exist");
    assert_eq!(sync_rec.child_rel_path, "Trip_A");
    assert!(sync_rec.bundle_count >= 1);
    assert!(!sync_rec.child_last_run_id.is_empty());

    // Run second parent rollup: should succeed and keep synced records
    drop(db);
    run_parent_rollup(&all_args, &child_dirs).expect("second parent rollup");

    let db2 = ManifestDb::open(&parent_manifest).expect("open parent db after second rollup");
    let sync_rec2 = db2
        .get_child_sync("Trip_A")
        .expect("get sync record")
        .expect("sync record must exist");
    assert_eq!(sync_rec2.child_last_run_id, sync_rec.child_last_run_id);
    assert_eq!(sync_rec2.bundle_count, sync_rec.bundle_count);
}

#[test]
fn test_parent_rollup_prunes_deleted_child() {
    let temp = tempdir().expect("create tempdir");
    let root = temp.path();

    let trip_a = root.join("Trip_A");
    let trip_b = root.join("Trip_B");
    fs::create_dir_all(&trip_a).expect("create trip_a");
    fs::create_dir_all(&trip_b).expect("create trip_b");
    fs::write(trip_a.join("IMG_0001.JPG"), b"fake_jpeg_1").expect("write img1");
    fs::write(trip_b.join("IMG_0002.JPG"), b"fake_jpeg_2").expect("write img2");

    let args_a = make_test_organize_args(trip_a.clone());
    run_organize(&args_a).expect("organize trip_a");
    let args_b = make_test_organize_args(trip_b.clone());
    run_organize(&args_b).expect("organize trip_b");

    let all_args = OrganizeAllArgs {
        input: root.to_path_buf(),
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

    let child_dirs_initial = vec![trip_a.clone(), trip_b.clone()];
    run_parent_rollup(&all_args, &child_dirs_initial).expect("initial rollup");

    let parent_manifest = root.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    let db = ManifestDb::open(&parent_manifest).expect("open parent db");
    assert!(db.get_child_sync("Trip_A").expect("sync").is_some());
    assert!(db.get_child_sync("Trip_B").expect("sync").is_some());
    let bundles_initial = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(bundles_initial.len(), 2);
    drop(db);

    // Remove trip_b from disk
    fs::remove_dir_all(&trip_b).expect("remove trip_b");

    // Run parent rollup with only remaining child dirs
    let child_dirs_remaining = vec![trip_a.clone()];
    run_parent_rollup(&all_args, &child_dirs_remaining).expect("rollup after trip_b deleted");

    let db2 = ManifestDb::open(&parent_manifest).expect("open parent db after deletion");
    assert!(db2.get_child_sync("Trip_A").expect("sync").is_some());
    assert!(
        db2.get_child_sync("Trip_B").expect("sync").is_none(),
        "Trip_B sync record must be pruned"
    );

    let bundles_after = db2.get_all_bundles(None).expect("get bundles");
    assert_eq!(
        bundles_after.len(),
        1,
        "Only Trip_A bundles must remain in parent DB"
    );
    assert_eq!(bundles_after[0].1.trip_folder_name, "Trip_A");

    // Parent catalog HTML must not contain Trip_B
    let html =
        fs::read_to_string(root.join(DEFAULT_CATALOG_FILENAME)).expect("read parent catalog");
    assert!(html.contains("Trip_A/avicull_catalog_files/previews/IMG_0001.jpg"));
    assert!(!html.contains("Trip_B/avicull_catalog_files/previews/IMG_0002.jpg"));
}
