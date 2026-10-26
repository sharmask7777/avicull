use avicull::db::ManifestDb;
use avicull::organizer::{
    clean_run_hardlinks, clean_run_operations, execute_inplace_bundle_rename,
    materialize_bundle_hardlinks,
};
use avicull::types::{AssetBundle, BundleDecision, RoutingTier, SpeciesPrediction};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;

#[test]
fn test_inplace_rename_recording_and_clean_rollback() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("trip_leh");
    fs::create_dir_all(&input_dir).expect("create input dir");

    // 1. Create mock files: RAW primary, .xmp sidecar, and companion JPEG
    let raw_path = input_dir.join("IMG_0001.CR2");
    let xmp_path = input_dir.join("IMG_0001.CR2.xmp");
    let jpg_path = input_dir.join("IMG_0001.JPG");

    let raw_payload = b"CR2_RAW_PAYLOAD_BYTES_LEH_2026";
    let xmp_payload = b"<xmpmeta>Leh rating=5 label=Red</xmpmeta>";
    let jpg_payload = b"JPEG_PREVIEW_PAYLOAD_LEH_2026";

    File::create(&raw_path)
        .unwrap()
        .write_all(raw_payload)
        .unwrap();
    File::create(&xmp_path)
        .unwrap()
        .write_all(xmp_payload)
        .unwrap();
    File::create(&jpg_path)
        .unwrap()
        .write_all(jpg_payload)
        .unwrap();

    let raw_ino = fs::metadata(&raw_path).unwrap().ino();
    let xmp_ino = fs::metadata(&xmp_path).unwrap().ino();
    let jpg_ino = fs::metadata(&jpg_path).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "bundle_img_0001".to_string(),
        trip_folder_name: "trip_leh".to_string(),
        original_stem: "IMG_0001".to_string(),
        primary_image: raw_path.clone(),
        companion_images: vec![jpg_path.clone()],
        sidecar_files: vec![xmp_path.clone()],
        capture_timestamp_ms: Some(1730000000000), // ~2024-10-27
        gps_coords: None,
        resolved_location_name: Some("Leh".to_string()),
        relative_path: None,
        source_inode: raw_ino,
    };

    // 2. Perform in-place bundle renaming
    let renames = execute_inplace_bundle_rename(&bundle, "Tibetan Snowcock", None, None)
        .expect("in-place rename succeeded");
    assert_eq!(renames.len(), 3);

    // Verify files were renamed on disk
    for (orig, renamed) in &renames {
        assert!(
            !orig.exists(),
            "Original {} should no longer exist",
            orig.display()
        );
        assert!(
            renamed.exists(),
            "Renamed {} must exist on disk",
            renamed.display()
        );
    }

    // 3. Open SQLite manifest database and record in-place renames
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db");
    let run_id = "run-inplace-rollback-1";

    db.record_run(
        run_id,
        &input_dir,
        &input_dir,
        false,
        r#"{"mode":"in_place"}"#,
    )
    .expect("record run");

    for (orig, renamed) in &renames {
        db.record_inplace_rename(run_id, orig, renamed)
            .expect("record_inplace_rename");
    }

    // 4. Run clean_run_operations to rollback in-place renames
    let reverted_count = clean_run_operations(&db, run_id).expect("clean_run_operations");
    assert_eq!(reverted_count, 3, "Expected 3 files rolled back");

    // 5. Verify all original files are 100% restored and content intact
    assert!(raw_path.exists(), "RAW file must be restored");
    assert!(xmp_path.exists(), "XMP file must be restored");
    assert!(jpg_path.exists(), "JPG file must be restored");

    assert_eq!(fs::read(&raw_path).unwrap(), raw_payload);
    assert_eq!(fs::read(&xmp_path).unwrap(), xmp_payload);
    assert_eq!(fs::read(&jpg_path).unwrap(), jpg_payload);

    assert_eq!(fs::metadata(&raw_path).unwrap().ino(), raw_ino);
    assert_eq!(fs::metadata(&xmp_path).unwrap().ino(), xmp_ino);
    assert_eq!(fs::metadata(&jpg_path).unwrap().ino(), jpg_ino);

    // Verify renamed files no longer exist
    for (_orig, renamed) in &renames {
        assert!(
            !renamed.exists(),
            "Renamed file {} should not exist after rollback",
            renamed.display()
        );
    }
}

#[test]
fn test_rollback_inplace_renames_direct_api() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("trip_direct");
    fs::create_dir_all(&input_dir).expect("create input dir");

    let f1 = input_dir.join("PHOTO_100.JPG");
    let f2 = input_dir.join("PHOTO_101.JPG");
    File::create(&f1).unwrap().write_all(b"PHOTO_100").unwrap();
    File::create(&f2).unwrap().write_all(b"PHOTO_101").unwrap();

    let r1 = input_dir.join("PHOTO_100_Robin.JPG");
    let r2 = input_dir.join("PHOTO_101_Sparrow.JPG");
    fs::rename(&f1, &r1).unwrap();
    fs::rename(&f2, &r2).unwrap();

    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db");
    let run_id = "run-direct-rollback";

    db.record_run(run_id, &input_dir, &input_dir, false, "{}")
        .unwrap();
    db.record_inplace_rename(run_id, &f1, &r1).unwrap();
    db.record_inplace_rename(run_id, &f2, &r2).unwrap();

    // Directly invoke ManifestDb::rollback_inplace_renames
    let rolled_back = db.rollback_inplace_renames(run_id).expect("rollback");
    assert_eq!(rolled_back, 2);

    assert!(f1.exists());
    assert!(f2.exists());
    assert!(!r1.exists());
    assert!(!r2.exists());
    assert_eq!(fs::read(&f1).unwrap(), b"PHOTO_100");
    assert_eq!(fs::read(&f2).unwrap(), b"PHOTO_101");
}

#[test]
fn test_clean_hardlink_operations_unlinks_without_affecting_source() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("source_photos");
    let output_dir = temp.path().join("birds_hierarchy");
    fs::create_dir_all(&input_dir).expect("create input dir");

    let src_photo = input_dir.join("DSC_9999.JPG");
    let payload = b"SOURCE_IMAGE_DATA_CANON";
    File::create(&src_photo)
        .unwrap()
        .write_all(payload)
        .unwrap();
    let src_ino = fs::metadata(&src_photo).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "b_9999".to_string(),
        trip_folder_name: "source_photos".to_string(),
        original_stem: "DSC_9999".to_string(),
        primary_image: src_photo.clone(),
        companion_images: Vec::new(),
        sidecar_files: Vec::new(),
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
        source_inode: src_ino,
    };

    let decision = BundleDecision {
        bundle_id: "b_9999".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "barswa".to_string(),
            common_name: "Barn Swallow".to_string(),
            folder_slug: "Barn_Swallow".to_string(),
            scientific_name: "Hirundo rustica".to_string(),
            visual_prob: 0.95,
            geo_prior: 1.0,
            combined_score: 0.95,
        }],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "Barn_Swallow".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db");
    let run_id = "run-hardlink-clean-1";

    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();
    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .unwrap();

    let ops = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, run_id, false).unwrap();
    db.record_hardlink_operations(&ops).unwrap();

    // Verify hardlink exists and points to same inode
    assert_eq!(ops.len(), 1);
    let linked_dest = &ops[0].dest_path;
    assert!(linked_dest.exists());
    assert_eq!(fs::metadata(linked_dest).unwrap().ino(), src_ino);
    assert_eq!(fs::metadata(&src_photo).unwrap().nlink(), 2);

    // Clean hardlinks via clean_run_hardlinks (which delegates to clean_run_operations)
    let unlinked = clean_run_hardlinks(&db, run_id).expect("clean hardlinks");
    assert_eq!(unlinked, 1);

    // Verify destination hardlink was unlinked
    assert!(!linked_dest.exists());

    // Verify SOURCE IS 100% UNTOUCHED
    assert!(src_photo.exists());
    assert_eq!(fs::metadata(&src_photo).unwrap().nlink(), 1);
    assert_eq!(fs::read(&src_photo).unwrap(), payload);
}
