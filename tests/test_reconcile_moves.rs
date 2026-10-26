use avicull::db::*;
use avicull::organizer::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_reconcile_manual_species_directory_move() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("input");
    let output_dir = dir.path().join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("DSC_0001.JPG");
    let src_xmp = input_dir.join("DSC_0001.xmp");
    {
        let mut f1 = File::create(&src_file).unwrap();
        f1.write_all(b"fake image bytes").unwrap();
        let mut f2 = File::create(&src_xmp).unwrap();
        f2.write_all(b"<x:xmpmeta/>").unwrap();
    }

    let src_ino = fs::metadata(&src_file).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "DSC_0001_bundle".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![src_xmp.clone()],
        source_inode: src_ino,
        capture_timestamp_ms: None,
        gps_coords: None,
        original_stem: "DSC_0001".to_string(),
        trip_folder_name: "trip1".to_string(),
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0001.JPG")),
    };

    let decision = BundleDecision {
        bundle_id: "DSC_0001_bundle".to_string(),
        burst_id: None,
        detection_confidence: 0.90,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.75,
        final_confidence: 0.75,
        assigned_species_slug: "Mallard".to_string(),
        routing_tier: RoutingTier::NeedsReview,
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

    let ops = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, run_id, false)
        .expect("materialize failed");
    db.record_hardlink_operations(&ops).unwrap();

    let initial_photo = output_dir.join("_Needs_Review/Mallard/DSC_0001_0.75.JPG");
    let initial_sidecar = output_dir.join("_Needs_Review/Mallard/DSC_0001_0.75.xmp");
    assert!(initial_photo.exists());
    assert!(initial_sidecar.exists());

    // 2. User manually moves DSC_0001 and its sidecar to birds_hierarchy/Northern_Pintail/
    let target_dir = output_dir.join("Northern_Pintail");
    fs::create_dir_all(&target_dir).unwrap();
    let moved_photo = target_dir.join("DSC_0001_0.75.JPG");
    let moved_sidecar = target_dir.join("DSC_0001_0.75.xmp");
    fs::rename(&initial_photo, &moved_photo).unwrap();
    fs::rename(&initial_sidecar, &moved_sidecar).unwrap();
    let _ = fs::remove_dir(output_dir.join("_Needs_Review/Mallard"));

    // 3. Run reconciliation
    let mut map = db.get_existing_bundle_map().unwrap();
    let reconciled_count =
        reconcile_manual_disk_moves(&output_dir, &db, &mut map).expect("reconciliation failed");
    assert_eq!(reconciled_count, 1);

    // 4. Verify in-memory map was updated
    let (mem_dec, _) = map.get("DSC_0001_bundle").unwrap();
    assert_eq!(mem_dec.assigned_species_slug, "Northern_Pintail");
    assert_eq!(mem_dec.routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(mem_dec.final_confidence, 1.0);
    assert!(mem_dec.is_user_override);

    // 5. Verify SQLite database was updated
    let fresh_map = db.get_existing_bundle_map().unwrap();
    let (db_dec, _) = fresh_map.get("DSC_0001_bundle").unwrap();
    assert_eq!(db_dec.assigned_species_slug, "Northern_Pintail");
    assert_eq!(db_dec.routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(db_dec.final_confidence, 1.0);
    assert!(db_dec.is_user_override);

    // 6. Verify hardlink operations destination paths were updated
    let can_target = target_dir.canonicalize().unwrap();
    let run_ops = db.get_run_operations(run_id).unwrap();
    assert_eq!(run_ops.len(), 2);
    for op in &run_ops {
        assert!(op
            .dest_path
            .canonicalize()
            .unwrap()
            .starts_with(&can_target));
        assert!(op.dest_path.exists());
    }
}

#[test]
fn test_reconcile_promote_from_needs_review_to_confirmed_same_species() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("input");
    let output_dir = dir.path().join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("DSC_0002.JPG");
    {
        let mut f1 = File::create(&src_file).unwrap();
        f1.write_all(b"fake image bytes").unwrap();
    }

    let src_ino = fs::metadata(&src_file).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "DSC_0002_bundle".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: src_ino,
        capture_timestamp_ms: None,
        gps_coords: None,
        original_stem: "DSC_0002".to_string(),
        trip_folder_name: "trip1".to_string(),
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0002.JPG")),
    };

    let decision = BundleDecision {
        bundle_id: "DSC_0002_bundle".to_string(),
        burst_id: None,
        detection_confidence: 0.90,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.70,
        final_confidence: 0.70,
        assigned_species_slug: "Mallard".to_string(),
        routing_tier: RoutingTier::NeedsReview,
        is_user_override: false,
    };

    let run_id = "run-002";
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();
    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .unwrap();

    let ops = materialize_bundle_hardlinks(&bundle, &decision, &output_dir, run_id, false)
        .expect("materialize failed");
    db.record_hardlink_operations(&ops).unwrap();

    let initial_photo = output_dir.join("_Needs_Review/Mallard/DSC_0002_0.70.JPG");
    assert!(initial_photo.exists());

    // User moves photo from _Needs_Review/Mallard to Mallard (confirming it without changing species)
    let target_dir = output_dir.join("Mallard");
    fs::create_dir_all(&target_dir).unwrap();
    let moved_photo = target_dir.join("DSC_0002_0.70.JPG");
    fs::rename(&initial_photo, &moved_photo).unwrap();

    let mut map = db.get_existing_bundle_map().unwrap();
    let count = reconcile_manual_disk_moves(&output_dir, &db, &mut map).unwrap();
    assert_eq!(count, 1);

    let (db_dec, _) = map.get("DSC_0002_bundle").unwrap();
    assert_eq!(db_dec.assigned_species_slug, "Mallard");
    assert_eq!(db_dec.routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(db_dec.final_confidence, 1.0);
    assert!(db_dec.is_user_override);

    // Second run should be a no-op
    let count2 = reconcile_manual_disk_moves(&output_dir, &db, &mut map).unwrap();
    assert_eq!(count2, 0);
}

#[test]
fn test_reconcile_manual_move_to_no_bird_detected() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("input");
    let output_dir = dir.path().join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("DSC_0003.JPG");
    {
        let mut f1 = File::create(&src_file).unwrap();
        f1.write_all(b"fake image bytes").unwrap();
    }

    let src_ino = fs::metadata(&src_file).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "DSC_0003_bundle".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: src_ino,
        capture_timestamp_ms: None,
        gps_coords: None,
        original_stem: "DSC_0003".to_string(),
        trip_folder_name: "trip1".to_string(),
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0003.JPG")),
    };

    let decision = BundleDecision {
        bundle_id: "DSC_0003_bundle".to_string(),
        burst_id: None,
        detection_confidence: 0.85,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "Mallard".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    let run_id = "run-003";
    let db = ManifestDb::open_or_create(&db_path).unwrap();
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

    let initial_photo = output_dir.join("Mallard/DSC_0003_0.95.JPG");
    assert!(initial_photo.exists());

    // User moves photo into _No_Bird_Detected
    let target_dir = output_dir.join("_No_Bird_Detected");
    fs::create_dir_all(&target_dir).unwrap();
    let moved_photo = target_dir.join("DSC_0003_0.95.JPG");
    fs::rename(&initial_photo, &moved_photo).unwrap();

    let mut map = db.get_existing_bundle_map().unwrap();
    let count = reconcile_manual_disk_moves(&output_dir, &db, &mut map).unwrap();
    assert_eq!(count, 1);

    let (db_dec, _) = map.get("DSC_0003_bundle").unwrap();
    assert_eq!(db_dec.assigned_species_slug, "_No_Bird_Detected");
    assert_eq!(db_dec.routing_tier, RoutingTier::NoBirdDetected);
    assert_eq!(db_dec.final_confidence, 1.0);
    assert!(db_dec.is_user_override);
}

#[test]
fn test_reconcile_stem_matching_when_inode_differs() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("input");
    let output_dir = dir.path().join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let src_file = input_dir.join("DSC_0004.JPG");
    {
        let mut f1 = File::create(&src_file).unwrap();
        f1.write_all(b"fake image bytes").unwrap();
    }

    // Set source_inode in bundle to a dummy value so inode matching fails and stem matching is used
    let bundle = AssetBundle {
        bundle_id: "DSC_0004_bundle".to_string(),
        primary_image: src_file.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: 9999999,
        capture_timestamp_ms: None,
        gps_coords: None,
        original_stem: "DSC_0004".to_string(),
        trip_folder_name: "trip1".to_string(),
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0004.JPG")),
    };

    let decision = BundleDecision {
        bundle_id: "DSC_0004_bundle".to_string(),
        burst_id: None,
        detection_confidence: 0.85,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.60,
        final_confidence: 0.60,
        assigned_species_slug: "Mallard".to_string(),
        routing_tier: RoutingTier::NeedsReview,
        is_user_override: false,
    };

    let run_id = "run-004";
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();
    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .unwrap();

    // Place a file directly in Northern_Pintail (simulating copying photo on another machine)
    let target_dir = output_dir.join("Northern_Pintail");
    fs::create_dir_all(&target_dir).unwrap();
    let copied_photo = target_dir.join("DSC_0004_0.60.JPG");
    {
        let mut f = File::create(&copied_photo).unwrap();
        f.write_all(b"copied bytes").unwrap();
    }

    let mut map = db.get_existing_bundle_map().unwrap();
    let count = reconcile_manual_disk_moves(&output_dir, &db, &mut map).unwrap();
    assert_eq!(count, 1);

    let (db_dec, _) = map.get("DSC_0004_bundle").unwrap();
    assert_eq!(db_dec.assigned_species_slug, "Northern_Pintail");
    assert_eq!(db_dec.routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(db_dec.final_confidence, 1.0);
    assert!(db_dec.is_user_override);
}

#[test]
fn test_reconcile_inplace_manual_filename_rename() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("photos");
    fs::create_dir_all(&input_dir).unwrap();

    let orig_photo = input_dir.join("DSC_0001.JPG");
    let orig_xmp = input_dir.join("DSC_0001.xmp");
    let initial_photo = input_dir.join("DSC_0001_Mallard.JPG");
    let initial_sidecar = input_dir.join("DSC_0001_Mallard.xmp");

    {
        let mut f1 = File::create(&initial_photo).unwrap();
        f1.write_all(b"fake image bytes").unwrap();
        let mut f2 = File::create(&initial_sidecar).unwrap();
        f2.write_all(b"<x:xmpmeta/>").unwrap();
    }

    let src_ino = fs::metadata(&initial_photo).unwrap().ino();

    let bundle = AssetBundle {
        bundle_id: "DSC_0001_bundle".to_string(),
        primary_image: orig_photo.clone(),
        companion_images: vec![],
        sidecar_files: vec![orig_xmp.clone()],
        source_inode: src_ino,
        capture_timestamp_ms: None,
        gps_coords: None,
        original_stem: "DSC_0001".to_string(),
        trip_folder_name: "photos".to_string(),
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0001.JPG")),
    };

    let decision = BundleDecision {
        bundle_id: "DSC_0001_bundle".to_string(),
        burst_id: None,
        detection_confidence: 0.90,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.85,
        final_confidence: 0.85,
        assigned_species_slug: "Mallard".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    let run_id = "run-001";
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    db.record_run(run_id, &input_dir, &input_dir, false, "{}")
        .unwrap();
    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .unwrap();
    db.record_inplace_rename(run_id, &orig_photo, &initial_photo)
        .unwrap();
    db.record_inplace_rename(run_id, &orig_xmp, &initial_sidecar)
        .unwrap();
    db.complete_run(run_id).unwrap();

    assert!(initial_photo.exists());
    assert!(initial_sidecar.exists());

    // User manually renames primary image from Mallard to Northern_Pintail
    let moved_photo = input_dir.join("DSC_0001_Northern_Pintail.JPG");
    fs::rename(&initial_photo, &moved_photo).unwrap();

    // Re-run organize in in-place mode
    let status = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place",
            "--no-prompt",
        ])
        .status()
        .expect("run avicull organize in-place");
    assert!(status.success());

    // Primary photo with new name should exist
    assert!(moved_photo.exists(), "Moved primary photo must exist");
    assert!(!initial_photo.exists(), "Old primary photo must not exist");

    // Companion sidecar should be automatically renamed to match
    let expected_sidecar = input_dir.join("DSC_0001_Northern_Pintail.xmp");
    assert!(
        expected_sidecar.exists(),
        "Sidecar file should be reconciled and renamed to Northern_Pintail"
    );
    assert!(
        !initial_sidecar.exists(),
        "Old sidecar DSC_0001_Mallard.xmp should no longer exist"
    );

    // Verify SQLite manifest was updated with user override
    let fresh_map = db.get_existing_bundle_map().unwrap();
    let (db_dec, _) = fresh_map
        .get("DSC_0001_bundle")
        .expect("bundle must exist in manifest");
    assert_eq!(db_dec.assigned_species_slug, "Northern_Pintail");
    assert_eq!(db_dec.routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(db_dec.final_confidence, 1.0);
    assert!(db_dec.is_user_override);

    // Verify bird_catalog.html was refreshed and contains Northern_Pintail
    let catalog_path = input_dir.join("bird_catalog.html");
    assert!(catalog_path.exists());
    let catalog_html = fs::read_to_string(&catalog_path).unwrap();
    assert!(catalog_html.contains("Northern_Pintail"));
}
