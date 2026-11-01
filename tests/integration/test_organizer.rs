use avicull::db::*;
use avicull::organizer::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;

#[test]
fn test_sanitize_species_slug() {
    assert_eq!(
        sanitize_species_slug("Blyth's Reed Warbler"),
        "Blyths_Reed_Warbler"
    );
    assert_eq!(
        sanitize_species_slug("Streak-throated Swallow"),
        "Streak-throated_Swallow"
    );
    assert_eq!(
        sanitize_species_slug("Great / Indian Bustard (Rare)"),
        "Great___Indian_Bustard__Rare_"
    );
    assert_eq!(
        sanitize_species_slug("pictures_bangalore-2025-"),
        "pictures_bangalore-2025"
    );
    assert_eq!(
        sanitize_species_slug("-trailing-and-leading-"),
        "trailing-and-leading"
    );
}

#[test]
fn test_format_destination_filename_with_date_and_location() {
    // Both date and location
    assert_eq!(
        format_destination_filename(
            "1X7A0791.jpg",
            0.98,
            None,
            Some("2025-01-26"),
            Some("Saul_Kere")
        ),
        "1X7A0791_2025-01-26_Saul_Kere_0.98.jpg"
    );
    // Compound sidecar with date and location
    assert_eq!(
        format_destination_filename(
            "1X7A0791.CR2.xmp",
            0.98,
            None,
            Some("2025-01-26"),
            Some("Saul_Kere")
        ),
        "1X7A0791_2025-01-26_Saul_Kere_0.98.CR2.xmp"
    );
    // Only date
    assert_eq!(
        format_destination_filename("1X7A0791.jpg", 0.98, None, Some("2025-01-26"), None),
        "1X7A0791_2025-01-26_0.98.jpg"
    );
    // Only location
    assert_eq!(
        format_destination_filename("1X7A0791.jpg", 0.98, None, None, Some("Saul_Kere")),
        "1X7A0791_Saul_Kere_0.98.jpg"
    );
    // Neither date nor location
    assert_eq!(
        format_destination_filename("1X7A0791.jpg", 0.98, None, None, None),
        "1X7A0791_0.98.jpg"
    );
    // With trip prefix collision disambiguation
    assert_eq!(
        format_destination_filename(
            "1X7A0791.jpg",
            0.98,
            Some("trip1"),
            Some("2025-01-26"),
            Some("Saul_Kere")
        ),
        "trip1__1X7A0791_2025-01-26_Saul_Kere_0.98.jpg"
    );
}

#[test]
fn test_format_destination_filename() {
    // Normal single extension
    assert_eq!(
        format_destination_filename("DSC_4011.JPG", 0.96, None, None, None),
        "DSC_4011_0.96.JPG"
    );
    // Lowercase extension preserved
    assert_eq!(
        format_destination_filename("DSC_4011.xmp", 0.96, None, None, None),
        "DSC_4011_0.96.xmp"
    );
    // Multi-extension sidecar: DSC_4011.JPG.xmp -> DSC_4011_0.96.JPG.xmp
    assert_eq!(
        format_destination_filename("DSC_4011.JPG.xmp", 0.96, None, None, None),
        "DSC_4011_0.96.JPG.xmp"
    );
    // Multi-extension RAW sidecar: DSC_4011.ARW.xmp -> DSC_4011_0.96.ARW.xmp
    assert_eq!(
        format_destination_filename("DSC_4011.ARW.xmp", 0.96, None, None, None),
        "DSC_4011_0.96.ARW.xmp"
    );
    // With trip prefix disambiguation
    assert_eq!(
        format_destination_filename("DSC_4011.JPG", 0.96, Some("2025_Bharatpur"), None, None),
        "2025_Bharatpur__DSC_4011_0.96.JPG"
    );
}

#[test]
fn test_materialize_hardlinks_and_idempotency_and_clean() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("trips");
    let output_dir = dir.path().join("by_species");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("DSC_4011.JPG");
    let sidecar_file = input_dir.join("DSC_4011.xmp");
    File::create(&src_file)
        .unwrap()
        .write_all(b"JPEG DATA")
        .unwrap();
    File::create(&sidecar_file)
        .unwrap()
        .write_all(b"XMP DATA")
        .unwrap();

    let src_meta = fs::metadata(&src_file).unwrap();
    let src_ino = src_meta.ino();

    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: src_ino,
        trip_folder_name: "2025-11_Bharatpur".to_string(),
        original_stem: "DSC_4011".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![sidecar_file.clone()],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: Some("burst-001".to_string()),
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    let run_id = "run-001";
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();
    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .unwrap();

    // 1. Materialize hard links
    let ops = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, run_id, false)
        .expect("materialize failed");
    db.record_hardlink_operations(&ops).unwrap();

    let expected_dst = output_dir.join("Painted_Stork/DSC_4011_0.96.JPG");
    let expected_sidecar = output_dir.join("Painted_Stork/DSC_4011_0.96.xmp");

    assert!(expected_dst.exists());
    assert!(expected_sidecar.exists());

    // Inode check: dst must share exact same inode as src, and nlink == 2
    let dst_meta = fs::metadata(&expected_dst).unwrap();
    assert_eq!(dst_meta.ino(), src_ino);
    assert_eq!(dst_meta.nlink(), 2);

    // 2. Run idempotency test: re-running on exact same files must succeed as no-op
    let ops2 = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, run_id, false)
        .expect("second run must be idempotent");
    assert_eq!(ops2[0].status, "already_linked");

    // 3. Clean test: unlinks generated files from output, restoring src nlink to 1
    let unlinked_count = clean_run_hardlinks(&db, run_id).expect("clean failed");
    assert_eq!(unlinked_count, 2);
    assert!(!expected_dst.exists());
    assert!(!expected_sidecar.exists());

    // Src file still exists untouched with nlink == 1
    let src_meta_after = fs::metadata(&src_file).unwrap();
    assert_eq!(src_meta_after.nlink(), 1);
}

#[test]
fn test_collision_across_trips_prefixes_trip_name() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("trips");
    let output_dir = dir.path().join("by_species");

    let trip1_dir = input_dir.join("2025_Trip1");
    let trip2_dir = input_dir.join("2026_Trip2");
    fs::create_dir_all(&trip1_dir).unwrap();
    fs::create_dir_all(&trip2_dir).unwrap();

    // Both trips have DSC_0001.JPG with different content (different inodes)
    let f1 = trip1_dir.join("DSC_0001.JPG");
    let f2 = trip2_dir.join("DSC_0001.JPG");
    File::create(&f1)
        .unwrap()
        .write_all(b"PHOTO FROM TRIP 1")
        .unwrap();
    File::create(&f2)
        .unwrap()
        .write_all(b"PHOTO FROM TRIP 2")
        .unwrap();

    let meta1 = fs::metadata(&f1).unwrap();
    let meta2 = fs::metadata(&f2).unwrap();
    assert_ne!(meta1.ino(), meta2.ino());

    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: meta1.ino(),
        trip_folder_name: "2025_Trip1".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: f1,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let b2 = AssetBundle {
        bundle_id: "b2".to_string(),
        source_inode: meta2.ino(),
        trip_folder_name: "2026_Trip2".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: f2,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "any".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    // Materialize Trip 1
    materialize_bundle_hardlinks(&b1, &decision, &output_dir, "run-1", false).unwrap();
    let dst1 = output_dir.join("Painted_Stork/DSC_0001_0.96.JPG");
    assert!(dst1.exists());

    // Materialize Trip 2: Collides at DSC_0001_0.96.JPG with different inode!
    // Must prefix with sanitized trip name: 2026_Trip2__DSC_0001_0.96.JPG
    materialize_bundle_hardlinks(&b2, &decision, &output_dir, "run-1", false).unwrap();
    let dst2_prefixed = output_dir.join("Painted_Stork/2026_Trip2__DSC_0001_0.96.JPG");
    assert!(
        dst2_prefixed.exists(),
        "Colliding file from Trip 2 must be prefixed with trip name"
    );
}

#[test]
fn test_numbered_counter_collision_resolution_on_duplicate_stems() {
    let dir = tempdir().expect("create temp dir");
    let trip_dir = dir.path().join("trip");
    let output_dir = dir.path().join("birds_hierarchy");
    fs::create_dir_all(&trip_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Two distinct files in subfolders that both have stem "IMG_9999" and same trip name
    let sub1 = trip_dir.join("sub1");
    let sub2 = trip_dir.join("sub2");
    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();

    let f1 = sub1.join("IMG_9999.JPG");
    let f2 = sub2.join("IMG_9999.JPG");
    File::create(&f1).unwrap().write_all(b"PHOTO 1").unwrap();
    File::create(&f2).unwrap().write_all(b"PHOTO 2").unwrap();

    let meta1 = fs::metadata(&f1).unwrap();
    let meta2 = fs::metadata(&f2).unwrap();
    assert_ne!(meta1.ino(), meta2.ino());

    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: meta1.ino(),
        trip_folder_name: "trip".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: f1,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let b2 = AssetBundle {
        bundle_id: "b2".to_string(),
        source_inode: meta2.ino(),
        trip_folder_name: "trip".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: f2,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "b".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.90,
        final_confidence: 0.90,
        assigned_species_slug: "Chukar".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    // Materialize first file: creates Chukar/IMG_9999_0.90.JPG
    materialize_bundle_hardlinks(&b1, &decision, &output_dir, "run-1", false).unwrap();
    let dst1 = output_dir.join("Chukar/IMG_9999_0.90.JPG");
    assert!(dst1.exists());
    assert_eq!(fs::metadata(&dst1).unwrap().ino(), meta1.ino());

    // Materialize second file with identical stem and same trip:
    // Primary path collides with different inode -> prefixes trip name: Chukar/trip__IMG_9999_0.90.JPG
    materialize_bundle_hardlinks(&b2, &decision, &output_dir, "run-1", false).unwrap();
    let dst2 = output_dir.join("Chukar/trip__IMG_9999_0.90.JPG");
    assert!(
        dst2.exists(),
        "Second colliding file must have trip prefix disambiguation"
    );
    assert_eq!(fs::metadata(&dst2).unwrap().ino(), meta2.ino());

    // Materialize third file (different inode, same stem, same trip):
    // Both primary AND trip-prefixed path collide -> appends _1: Chukar/trip__IMG_9999_0.90_1.JPG
    let sub3 = trip_dir.join("sub3");
    fs::create_dir_all(&sub3).unwrap();
    let f3 = sub3.join("IMG_9999.JPG");
    File::create(&f3).unwrap().write_all(b"PHOTO 3").unwrap();
    let meta3 = fs::metadata(&f3).unwrap();

    let b3 = AssetBundle {
        bundle_id: "b3".to_string(),
        source_inode: meta3.ino(),
        trip_folder_name: "trip".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: f3,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };
    materialize_bundle_hardlinks(&b3, &decision, &output_dir, "run-1", false).unwrap();
    let dst3 = output_dir.join("Chukar/trip__IMG_9999_0.90_1.JPG");
    assert!(
        dst3.exists(),
        "Third colliding file must receive numbered counter _1"
    );
    assert_eq!(fs::metadata(&dst3).unwrap().ino(), meta3.ino());

    // Materialize fourth file: must receive _2, NOT _1_2!
    let sub4 = trip_dir.join("sub4");
    fs::create_dir_all(&sub4).unwrap();
    let f4 = sub4.join("IMG_9999.JPG");
    File::create(&f4).unwrap().write_all(b"PHOTO 4").unwrap();
    let meta4 = fs::metadata(&f4).unwrap();

    let b4 = AssetBundle {
        bundle_id: "b4".to_string(),
        source_inode: meta4.ino(),
        trip_folder_name: "trip".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: f4,
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };
    materialize_bundle_hardlinks(&b4, &decision, &output_dir, "run-1", false).unwrap();
    let dst4 = output_dir.join("Chukar/trip__IMG_9999_0.90_2.JPG");
    assert!(
        dst4.exists(),
        "Fourth colliding file must receive clean numbered counter _2"
    );
    assert_eq!(fs::metadata(&dst4).unwrap().ino(), meta4.ino());
}

#[test]
fn test_materialize_hardlinks_with_date_and_location() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("trips");
    let output_dir = dir.path().join("by_species");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("1X7A0791.JPG");
    let sidecar_file = input_dir.join("1X7A0791.xmp");
    File::create(&src_file)
        .unwrap()
        .write_all(b"JPEG DATA")
        .unwrap();
    File::create(&sidecar_file)
        .unwrap()
        .write_all(b"XMP DATA")
        .unwrap();

    let src_meta = fs::metadata(&src_file).unwrap();
    let src_ino = src_meta.ino();

    // 1737888000000 ms is 2025-01-26 10:40:00 UTC
    let bundle = AssetBundle {
        bundle_id: "b_annotated".to_string(),
        source_inode: src_ino,
        trip_folder_name: "Saul Kere jan 2025".to_string(),
        original_stem: "1X7A0791".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![sidecar_file.clone()],
        capture_timestamp_ms: Some(1737888000000),
        gps_coords: None,
        resolved_location_name: Some("Saul Kere".to_string()),
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "b_annotated".to_string(),
        burst_id: None,
        detection_confidence: 0.98,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.98,
        final_confidence: 0.98,
        assigned_species_slug: "Brahminy_Kite".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    let ops = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, "run-annotated", false)
        .unwrap();
    assert_eq!(ops.len(), 2);

    let expected_dst = output_dir.join("Brahminy_Kite/1X7A0791_2025-01-26_Saul_Kere_0.98.JPG");
    let expected_sidecar = output_dir.join("Brahminy_Kite/1X7A0791_2025-01-26_Saul_Kere_0.98.xmp");

    assert!(
        expected_dst.exists(),
        "Annotated destination image must exist"
    );
    assert!(
        expected_sidecar.exists(),
        "Annotated destination sidecar must exist"
    );
    assert_eq!(fs::metadata(&expected_dst).unwrap().ino(), src_ino);
}
