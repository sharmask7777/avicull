use avicull::organizer::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_format_inplace_filename_basic() {
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );

    // Compound sidecar extension
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            "CR2.xmp"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp"
    );

    // Extension with leading dot
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            ".CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );
}

#[test]
fn test_format_inplace_filename_date_and_location_omission() {
    // Both omitted
    assert_eq!(
        format_inplace_filename("IMG_4821", "House Sparrow", None, None, "CR2"),
        "IMG_4821_House_Sparrow.CR2"
    );

    // Empty strings omitted
    assert_eq!(
        format_inplace_filename("IMG_4821", "House Sparrow", Some(""), Some("   "), "CR2"),
        "IMG_4821_House_Sparrow.CR2"
    );

    // Unknown sentinels cleanly omitted
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("UnknownDate"),
            Some("UnknownLocation"),
            "jpg"
        ),
        "IMG_4821_House_Sparrow.jpg"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("Unknown"),
            Some("Unknown"),
            "jpg"
        ),
        "IMG_4821_House_Sparrow.jpg"
    );

    // Date formatted with hyphens normalized to YYYYMMDD
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "House Sparrow",
            Some("2026-10-25"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );
}

#[test]
fn test_format_inplace_filename_routing_tiers() {
    // Needs Review
    assert_eq!(
        format_inplace_filename(
            "IMG_4822",
            "Needs_Review_Black_Redstart",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh.CR2"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4822",
            "_Needs_Review_Black_Redstart",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh.CR2"
    );

    // No Bird Detected
    assert_eq!(
        format_inplace_filename(
            "IMG_4823",
            "No Bird Detected",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4823_No_Bird_Detected_20261025_Leh.CR2"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4823",
            "_No_Bird_Detected",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4823_No_Bird_Detected_20261025_Leh.CR2"
    );
}

#[test]
fn test_is_already_inplace_named() {
    assert!(is_already_inplace_named(
        "IMG_4821_House_Sparrow_20261025_Leh",
        "House Sparrow"
    ));
    assert!(is_already_inplace_named(
        "IMG_4821_House_Sparrow",
        "House Sparrow"
    ));
    assert!(is_already_inplace_named(
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh",
        "Black Redstart"
    ));
    assert!(is_already_inplace_named(
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh",
        "Needs_Review_Black_Redstart"
    ));
    assert!(is_already_inplace_named(
        "IMG_4823_No_Bird_Detected_20261025_Leh",
        "No Bird Detected"
    ));
    assert!(is_already_inplace_named(
        "IMG_4823_No_Bird_Detected_20261025_Leh",
        "_No_Bird_Detected"
    ));

    assert!(!is_already_inplace_named("IMG_4821", "House Sparrow"));
    assert!(!is_already_inplace_named("DSCRuff01", "Ruff"));
    assert!(!is_already_inplace_named(
        "IMG_4821_House_Sparrow_20261025_Leh",
        "Tibetan Snowcock"
    ));
}

#[test]
fn test_execute_inplace_bundle_rename_lockstep_all_files() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let primary = root.join("IMG_4821.CR2");
    let companion = root.join("IMG_4821.JPG");
    let sidecar1 = root.join("IMG_4821.xmp");
    let sidecar2 = root.join("IMG_4821.CR2.xmp");

    File::create(&primary)
        .unwrap()
        .write_all(b"raw-data")
        .unwrap();
    File::create(&companion)
        .unwrap()
        .write_all(b"jpeg-data")
        .unwrap();
    File::create(&sidecar1)
        .unwrap()
        .write_all(b"xmp-1")
        .unwrap();
    File::create(&sidecar2)
        .unwrap()
        .write_all(b"xmp-2")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle1".to_string(),
        source_inode: 12345,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![companion.clone()],
        sidecar_files: vec![sidecar1.clone(), sidecar2.clone()],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Rename must succeed");

    assert_eq!(renames.len(), 4);

    let expected_primary = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    let expected_companion = root.join("IMG_4821_House_Sparrow_20261025_Leh.JPG");
    let expected_sidecar1 = root.join("IMG_4821_House_Sparrow_20261025_Leh.xmp");
    let expected_sidecar2 = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp");

    assert!(expected_primary.exists(), "Primary must be renamed");
    assert!(expected_companion.exists(), "Companion must be renamed");
    assert!(expected_sidecar1.exists(), "Sidecar 1 must be renamed");
    assert!(expected_sidecar2.exists(), "Sidecar 2 must be renamed");

    assert!(!primary.exists(), "Old primary must not exist");
    assert!(!companion.exists(), "Old companion must not exist");
    assert!(!sidecar1.exists(), "Old sidecar 1 must not exist");
    assert!(!sidecar2.exists(), "Old sidecar 2 must not exist");

    assert_eq!(fs::read(&expected_primary).unwrap(), b"raw-data");
    assert_eq!(fs::read(&expected_companion).unwrap(), b"jpeg-data");
    assert_eq!(fs::read(&expected_sidecar1).unwrap(), b"xmp-1");
    assert_eq!(fs::read(&expected_sidecar2).unwrap(), b"xmp-2");
}

#[test]
fn test_execute_inplace_bundle_rename_idempotency_skip() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let primary = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    File::create(&primary)
        .unwrap()
        .write_all(b"raw-data")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle1".to_string(),
        source_inode: 12345,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821_House_Sparrow_20261025_Leh".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Should succeed without error");

    assert!(renames.is_empty(), "Idempotent rename must skip");
    assert!(primary.exists(), "Original file must remain untouched");
}

#[test]
fn test_execute_inplace_bundle_rename_collision_disambiguation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let existing_target = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    File::create(&existing_target)
        .unwrap()
        .write_all(b"existing-target")
        .unwrap();

    let primary = root.join("IMG_4821.CR2");
    File::create(&primary)
        .unwrap()
        .write_all(b"new-raw-data")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle2".to_string(),
        source_inode: 54321,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Collision disambiguation must succeed");

    assert_eq!(renames.len(), 1);
    let disambiguated_target = root.join("IMG_4821_House_Sparrow_20261025_Leh_1.CR2");
    assert!(disambiguated_target.exists());
    assert_eq!(fs::read(&disambiguated_target).unwrap(), b"new-raw-data");
    assert_eq!(fs::read(&existing_target).unwrap(), b"existing-target");
}
