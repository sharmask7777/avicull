use avicull::db::*;
use avicull::types::*;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_db_lifecycle_record_and_query() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");

    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let run_id = "run-001";
    db.record_run(
        run_id,
        &PathBuf::from("/photos/Trips"),
        &PathBuf::from("/photos/By_Species"),
        false,
        "{}",
    )
    .expect("record_run failed");

    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 999,
        trip_folder_name: "2025-11_Bharatpur".to_string(),
        original_stem: "DSC_4011".to_string(),
        primary_image: PathBuf::from("/photos/Trips/DSC_4011.JPG"),
        companion_images: vec![],
        sidecar_files: vec![PathBuf::from("/photos/Trips/DSC_4011.xmp")],
        capture_timestamp_ms: Some(1730000000000),
        gps_coords: Some(GpsCoordinate {
            latitude: 27.17,
            longitude: 77.52,
            source: LocationSource::ExifGps,
        }),
        resolved_location_name: None,
    };

    let decision = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: Some("burst-001".to_string()),
        detection_confidence: 0.95,
        bbox_xyxy_norm: Some([0.1, 0.1, 0.9, 0.9]),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "paisto1".to_string(),
            common_name: "Painted Stork".to_string(),
            folder_slug: "Painted_Stork".to_string(),
            scientific_name: "Mycteria leucocephala".to_string(),
            visual_prob: 0.95,
            geo_prior: 0.98,
            combined_score: 0.96,
        }],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
    };

    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .expect("record_bundles failed");

    let op = HardlinkOperation {
        op_id: None,
        run_id: run_id.to_string(),
        bundle_id: "b1".to_string(),
        source_path: PathBuf::from("/photos/Trips/DSC_4011.JPG"),
        dest_path: PathBuf::from("/photos/By_Species/Painted_Stork/DSC_4011_0.96.JPG"),
        source_inode: 999,
        status: "linked".to_string(),
        executed_at: Some("2026-10-24T12:00:00Z".to_string()),
    };

    db.record_hardlink_operations(std::slice::from_ref(&op))
        .expect("record_hardlink_operations failed");

    // Query operations back
    let ops = db
        .get_run_operations(run_id)
        .expect("get_run_operations failed");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].dest_path, op.dest_path);
    assert_eq!(ops[0].status, "linked");

    // Query bundles for relink
    let loaded = db
        .get_all_bundles(Some(run_id))
        .expect("get_all_bundles failed");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0.assigned_species_slug, "Painted_Stork");
    assert_eq!(loaded[0].1.original_stem, "DSC_4011");
}

#[test]
fn test_update_bundle_routing_tier() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let run_id = "run-002";
    db.record_run(
        run_id,
        &PathBuf::from("/photos/Trips"),
        &PathBuf::from("/photos/By_Species"),
        false,
        "{}",
    )
    .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle_xyz".to_string(),
        source_inode: 1234,
        trip_folder_name: "Trip".to_string(),
        original_stem: "IMG_001".to_string(),
        primary_image: PathBuf::from("/photos/Trips/IMG_001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
    };

    let decision = BundleDecision {
        bundle_id: "bundle_xyz".to_string(),
        burst_id: None,
        detection_confidence: 0.85,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.60,
        final_confidence: 0.60,
        assigned_species_slug: "Common_Merganser".to_string(),
        routing_tier: RoutingTier::NeedsReview, // Initially NeedsReview
    };

    db.record_bundles(run_id, &[decision], &[bundle]).unwrap();

    // Verify initial routing tier
    let initial = db.get_all_bundles(Some(run_id)).unwrap();
    assert_eq!(initial[0].0.routing_tier, RoutingTier::NeedsReview);

    // Update to ConfirmedSpecies (e.g. after relink with lower threshold)
    db.update_bundle_routing_tier("bundle_xyz", RoutingTier::ConfirmedSpecies)
        .unwrap();

    let updated = db.get_all_bundles(Some(run_id)).unwrap();
    assert_eq!(updated[0].0.routing_tier, RoutingTier::ConfirmedSpecies);
}
