//! Integration tests for unhidden catalog directory (`avicull_catalog_files`),
//! traversal scanners, and backward-compatible manifest resolution.

use avicull::catalog::{
    preview_path_and_name, resolve_manifest_db_path, resolve_operating_root, CATALOG_FILES_DIR,
};
use avicull::organizer::batch::should_skip_directory;
use avicull::scanner::{scan_input_tree, should_skip_dir};
use avicull::types::OrganizationMode;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_catalog_files_dir_constant_and_scanner_skips() {
    // 1. Verify unhidden catalog directory constant value
    assert_eq!(
        CATALOG_FILES_DIR, "avicull_catalog_files",
        "CATALOG_FILES_DIR must be 'avicull_catalog_files'"
    );

    // 2. Verify should_skip_directory in organizer::batch skips both
    assert!(
        should_skip_directory("avicull_catalog_files"),
        "should_skip_directory must skip 'avicull_catalog_files'"
    );
    assert!(
        should_skip_directory(".avicull"),
        "should_skip_directory must skip legacy '.avicull'"
    );
    assert!(
        should_skip_directory("birds_hierarchy"),
        "should_skip_directory must skip 'birds_hierarchy'"
    );
    assert!(
        !should_skip_directory("2024_05_Kaziranga"),
        "should_skip_directory must not skip legitimate trip subdirectories"
    );

    // 3. Verify should_skip_dir in scanner skips both
    assert!(
        should_skip_dir("avicull_catalog_files"),
        "should_skip_dir must skip 'avicull_catalog_files'"
    );
    assert!(
        should_skip_dir(".avicull"),
        "should_skip_dir must skip legacy '.avicull'"
    );
    assert!(
        !should_skip_dir("2024_05_Kaziranga"),
        "should_skip_dir must not skip legitimate trip subdirectories"
    );
}

#[test]
fn test_scanner_traversal_skips_catalog_directories() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    // Create valid photo in normal directory
    let trip_dir = root.join("TripA");
    fs::create_dir_all(&trip_dir).expect("create TripA");
    fs::write(trip_dir.join("IMG_0001.JPG"), b"fake jpeg content").expect("write photo");

    // Create assets inside avicull_catalog_files
    let unhidden_catalog = root.join(CATALOG_FILES_DIR).join("previews");
    fs::create_dir_all(&unhidden_catalog).expect("create avicull_catalog_files previews");
    fs::write(unhidden_catalog.join("IMG_0001.jpg"), b"preview").expect("write preview");

    // Create assets inside legacy .avicull
    let legacy_catalog = root.join(".avicull").join("previews");
    fs::create_dir_all(&legacy_catalog).expect("create legacy .avicull previews");
    fs::write(legacy_catalog.join("IMG_0001.jpg"), b"legacy preview")
        .expect("write legacy preview");

    // Scan input tree
    let bundles = scan_input_tree(root).expect("scan input tree");

    // Assert only the valid photo was scanned, catalog previews were skipped
    assert_eq!(
        bundles.len(),
        1,
        "Only photo in TripA should be discovered, skipping both catalog dirs"
    );
    assert_eq!(
        bundles[0].original_stem, "IMG_0001",
        "Scanned bundle should be IMG_0001"
    );
    let can_trip_dir = trip_dir.canonicalize().expect("canonicalize trip_dir");
    assert!(
        bundles[0].primary_image.starts_with(&can_trip_dir),
        "Scanned photo must reside in TripA, not in catalog files directories"
    );
}

#[test]
fn test_preview_target_path_defaults_to_unhidden_catalog_files() {
    let root = Path::new("/archive/trip1");
    let (preview_name, target_file) = preview_path_and_name(root, "IMG_5500");

    assert_eq!(preview_name, "IMG_5500.jpg");
    assert_eq!(
        target_file,
        PathBuf::from("/archive/trip1/avicull_catalog_files/previews/IMG_5500.jpg")
    );
}

#[test]
fn test_backward_compatible_manifest_db_path_resolution() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    // Case 1: Fresh directory (neither exists) -> defaults to avicull_catalog_files/manifest.sqlite
    let resolved =
        resolve_manifest_db_path(None, None, Some(root), Some(OrganizationMode::InPlace));
    assert_eq!(
        resolved,
        root.join("avicull_catalog_files/manifest.sqlite"),
        "Fresh directory should default to avicull_catalog_files/manifest.sqlite"
    );

    // Case 2: Only legacy .avicull/manifest.sqlite exists -> discovers legacy .avicull
    let legacy_db = root.join(".avicull/manifest.sqlite");
    fs::create_dir_all(legacy_db.parent().expect("legacy parent")).expect("create legacy dir");
    fs::write(&legacy_db, b"sqlite dummy").expect("write legacy sqlite");

    let resolved_legacy =
        resolve_manifest_db_path(None, None, Some(root), Some(OrganizationMode::InPlace));
    assert_eq!(
        resolved_legacy, legacy_db,
        "Should discover legacy .avicull/manifest.sqlite when only it exists"
    );

    // Case 3: Both exist -> avicull_catalog_files takes precedence
    let unhidden_db = root.join("avicull_catalog_files/manifest.sqlite");
    fs::create_dir_all(unhidden_db.parent().expect("unhidden parent"))
        .expect("create unhidden dir");
    fs::write(&unhidden_db, b"sqlite dummy").expect("write unhidden sqlite");

    let resolved_precedence =
        resolve_manifest_db_path(None, None, Some(root), Some(OrganizationMode::InPlace));
    assert_eq!(
        resolved_precedence, unhidden_db,
        "avicull_catalog_files/manifest.sqlite should take precedence when both exist"
    );

    // Case 4: Explicit --db flag always takes precedence
    let custom_db = root.join("custom/my_db.sqlite");
    let resolved_explicit = resolve_manifest_db_path(
        Some(&custom_db),
        None,
        Some(root),
        Some(OrganizationMode::InPlace),
    );
    assert_eq!(
        resolved_explicit, custom_db,
        "Explicit --db argument must take absolute precedence"
    );
}

#[test]
fn test_resolve_operating_root_strips_catalog_dir() {
    let root = Path::new("/Volumes/Photos/Kaziranga");

    // Unhidden catalog files dir
    let unhidden_db = root.join("avicull_catalog_files/manifest.sqlite");
    assert_eq!(resolve_operating_root(&unhidden_db), root);

    // Legacy .avicull dir
    let legacy_db = root.join(".avicull/manifest.sqlite");
    assert_eq!(resolve_operating_root(&legacy_db), root);

    // Relative path unhidden
    let rel_unhidden = Path::new("avicull_catalog_files/manifest.sqlite");
    assert_eq!(resolve_operating_root(rel_unhidden), Path::new("."));

    // Relative path legacy
    let rel_legacy = Path::new(".avicull/manifest.sqlite");
    assert_eq!(resolve_operating_root(rel_legacy), Path::new("."));
}
