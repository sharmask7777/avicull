//! Tests for re-evaluation of mock-failed and location-updated bundles during organize.

use avicull::cli::{Cli, Commands};
use avicull::organizer::should_reclassify_bundle;
use avicull::types::{
    AssetBundle, BundleDecision, GpsCoordinate, LocationSource, RoutingTier, SpeciesPrediction,
};
use clap::Parser;
use std::path::PathBuf;

fn create_sample_bundle(bundle_id: &str, coords: Option<GpsCoordinate>) -> AssetBundle {
    AssetBundle {
        bundle_id: bundle_id.to_string(),
        primary_image: PathBuf::from(format!("source/{bundle_id}.CR2")),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: 12345,
        capture_timestamp_ms: Some(1_700_000_000_000),
        gps_coords: coords,
        trip_folder_name: "Leh".to_string(),
        original_stem: bundle_id.to_string(),
        relative_path: Some(PathBuf::from(format!("source/{bundle_id}.CR2"))),
        resolved_location_name: None,
    }
}

fn create_valid_decision(bundle_id: &str, is_override: bool) -> BundleDecision {
    BundleDecision {
        bundle_id: bundle_id.to_string(),
        burst_id: Some("burst_1".to_string()),
        detection_confidence: 0.92,
        bbox_xyxy_norm: Some([0.1, 0.1, 0.8, 0.8]),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "houspa".to_string(),
            common_name: "House Sparrow".to_string(),
            folder_slug: "House_Sparrow".to_string(),
            scientific_name: "Passer domesticus".to_string(),
            visual_prob: 0.95,
            geo_prior: 1.0,
            combined_score: 0.95,
        }],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "House_Sparrow".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: is_override,
    }
}

fn create_mock_failed_decision(bundle_id: &str) -> BundleDecision {
    BundleDecision {
        bundle_id: bundle_id.to_string(),
        burst_id: None,
        detection_confidence: 0.0,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.0,
        final_confidence: 0.0,
        assigned_species_slug: RoutingTier::DIR_NO_BIRD_DETECTED.to_string(),
        routing_tier: RoutingTier::NoBirdDetected,
        is_user_override: false,
    }
}

#[test]
fn test_user_override_preserved_even_with_force_reclassify() {
    let bundle = create_sample_bundle("img_1", None);
    let decision = create_valid_decision("img_1", true);

    // Even with force_reclassify = true, user override must never be reclassified
    let reclassify = should_reclassify_bundle(&bundle, &decision, &bundle, true);
    assert!(
        !reclassify,
        "User override must never be reclassified even if force_reclassify is true"
    );

    // Even with coordinates updated, user override must remain protected
    let updated_bundle = create_sample_bundle(
        "img_1",
        Some(GpsCoordinate {
            latitude: 34.15,
            longitude: 77.57,
            source: LocationSource::SubdirectoryGazetteer,
        }),
    );
    let reclassify_coords = should_reclassify_bundle(&updated_bundle, &decision, &bundle, true);
    assert!(
        !reclassify_coords,
        "User override must never be reclassified even if coordinates change"
    );
}

#[test]
fn test_force_reclassify_triggers_on_normal_decision() {
    let bundle = create_sample_bundle("img_2", None);
    let decision = create_valid_decision("img_2", false);

    let reclassify = should_reclassify_bundle(&bundle, &decision, &bundle, true);
    assert!(
        reclassify,
        "force_reclassify should trigger reclassification for non-override bundles"
    );
}

#[test]
fn test_mock_failed_artifact_cached_when_reclassify_not_requested() {
    let bundle = create_sample_bundle("img_3", None);
    let decision = create_mock_failed_decision("img_3");

    let reclassify = should_reclassify_bundle(&bundle, &decision, &bundle, false);
    assert!(
        !reclassify,
        "Mock-failed bundles must remain cached when --reclassify is not requested"
    );
}

#[test]
fn test_mock_failed_artifact_triggers_when_force_reclassify_requested() {
    let bundle = create_sample_bundle("img_4", None);
    let decision = create_mock_failed_decision("img_4");

    let reclassify = should_reclassify_bundle(&bundle, &decision, &bundle, true);
    assert!(
        reclassify,
        "Mock-failed bundles must be reclassified when --reclassify is requested"
    );
}

#[test]
fn test_coordinate_update_triggers_reclassification() {
    let old_bundle = create_sample_bundle("img_5", None);
    let new_bundle = create_sample_bundle(
        "img_5",
        Some(GpsCoordinate {
            latitude: 34.1526,
            longitude: 77.5771,
            source: LocationSource::SubdirectoryGazetteer,
        }),
    );
    let decision = create_valid_decision("img_5", false);

    let reclassify = should_reclassify_bundle(&new_bundle, &decision, &old_bundle, false);
    assert!(
        reclassify,
        "Bundle with newly resolved coordinates must be reclassified"
    );
}

#[test]
fn test_coordinate_change_between_different_locations_triggers_reclassification() {
    let old_bundle = create_sample_bundle(
        "img_6",
        Some(GpsCoordinate {
            latitude: 28.6139,
            longitude: 77.2090,
            source: LocationSource::UserDefaultRegion,
        }),
    );
    let new_bundle = create_sample_bundle(
        "img_6",
        Some(GpsCoordinate {
            latitude: 34.1526,
            longitude: 77.5771,
            source: LocationSource::SubdirectoryGazetteer,
        }),
    );
    let decision = create_valid_decision("img_6", false);

    let reclassify = should_reclassify_bundle(&new_bundle, &decision, &old_bundle, false);
    assert!(
        reclassify,
        "Bundle with changed coordinates must be reclassified"
    );
}

#[test]
fn test_untouched_bundle_with_valid_decision_and_unchanged_coords_returns_false() {
    let coord = Some(GpsCoordinate {
        latitude: 34.1526,
        longitude: 77.5771,
        source: LocationSource::ExifGps,
    });
    let bundle = create_sample_bundle("img_7", coord);
    let decision = create_valid_decision("img_7", false);

    let reclassify = should_reclassify_bundle(&bundle, &decision, &bundle, false);
    assert!(
        !reclassify,
        "Untouched bundle with valid decision and matching coords should not be reclassified"
    );
}

#[test]
fn test_cli_organize_args_parses_reclassify_flag() {
    let args_with_flag = vec!["avicull", "organize", "--input", "photos", "--reclassify"];
    let parsed = Cli::try_parse_from(args_with_flag).expect("Should parse organize --reclassify");
    if let Commands::Organize(org_args) = parsed.command {
        assert!(org_args.reclassify, "--reclassify should be true when set");
    } else {
        panic!("Expected Organize command");
    }

    let args_without_flag = vec!["avicull", "organize", "--input", "photos"];
    let parsed_default =
        Cli::try_parse_from(args_without_flag).expect("Should parse organize without flag");
    if let Commands::Organize(org_args) = parsed_default.command {
        assert!(
            !org_args.reclassify,
            "--reclassify should default to false when omitted"
        );
    } else {
        panic!("Expected Organize command");
    }
}

#[test]
fn test_end_to_end_manifest_reclassification_detection() {
    use avicull::db::ManifestDb;
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("ManifestDb open");
    db.record_run("run-1", dir.path(), dir.path(), false, "{}")
        .expect("record run");

    // 1. Initial bundle recorded without GPS coordinates
    let bundle_v1 = create_sample_bundle("photo_100", None);
    let decision_v1 = create_valid_decision("photo_100", false);
    db.record_bundles_with_locality(
        "run-1",
        std::slice::from_ref(&decision_v1),
        std::slice::from_ref(&bundle_v1),
        &[false],
    )
    .expect("record v1");

    let existing_map = db.get_existing_bundle_map().expect("get bundle map");
    let (cached_dec, cached_bundle) = existing_map
        .get_by_bundle(&bundle_v1)
        .expect("Bundle must be found in manifest");

    // 2. Unchanged bundle should NOT be reclassified
    assert!(!should_reclassify_bundle(
        &bundle_v1,
        cached_dec,
        cached_bundle,
        false
    ));

    // 3. User override bundle should NOT be reclassified even with force
    let mut decision_override = decision_v1.clone();
    decision_override.is_user_override = true;
    db.record_bundles_with_locality(
        "run-1",
        std::slice::from_ref(&decision_override),
        std::slice::from_ref(&bundle_v1),
        &[false],
    )
    .expect("record override");
    let existing_override_map = db.get_existing_bundle_map().expect("get bundle map");
    let (cached_override_dec, cached_override_bundle) = existing_override_map
        .get_by_bundle(&bundle_v1)
        .expect("Bundle must be found");
    assert!(!should_reclassify_bundle(
        &bundle_v1,
        cached_override_dec,
        cached_override_bundle,
        true
    ));

    // 4. Coordinates updated in catalog/EXIF -> bundle should be reclassified
    let bundle_v2 = create_sample_bundle(
        "photo_100",
        Some(GpsCoordinate {
            latitude: 34.1526,
            longitude: 77.5771,
            source: LocationSource::SubdirectoryGazetteer,
        }),
    );
    // Note: non-override decision
    db.record_bundles_with_locality(
        "run-1",
        std::slice::from_ref(&decision_v1),
        std::slice::from_ref(&bundle_v1),
        &[false],
    )
    .expect("restore non-override");
    let restored_map = db.get_existing_bundle_map().expect("get bundle map");
    let (restored_dec, restored_bundle) = restored_map.get_by_bundle(&bundle_v2).expect("found");
    assert!(should_reclassify_bundle(
        &bundle_v2,
        restored_dec,
        restored_bundle,
        false
    ));
}

#[test]
fn test_discovered_bundle_preserves_cached_metadata_from_existing_bundle_map() {
    let cached_coords = Some(GpsCoordinate {
        latitude: 12.9716,
        longitude: 77.5946,
        source: LocationSource::SubdirectoryGazetteer,
    });
    let cached_ts = Some(1_700_000_000_123);
    let cached_loc_name = Some("Bangalore".to_string());

    let old_bundle = AssetBundle {
        bundle_id: "photo_preserved_1".to_string(),
        primary_image: PathBuf::from("source/photo_preserved_1.jpg"),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: 88888,
        capture_timestamp_ms: cached_ts,
        gps_coords: cached_coords,
        trip_folder_name: "Bangalore".to_string(),
        original_stem: "photo_preserved_1".to_string(),
        relative_path: Some(PathBuf::from("Bangalore/photo_preserved_1.jpg")),
        resolved_location_name: cached_loc_name.clone(),
    };
    let decision = create_valid_decision("photo_preserved_1", false);

    // Newly scanned bundle from scanner has None for EXIF/GPS/location
    let mut scanned_bundle = AssetBundle {
        bundle_id: "photo_preserved_1".to_string(),
        primary_image: PathBuf::from("source/photo_preserved_1.jpg"),
        companion_images: vec![],
        sidecar_files: vec![],
        source_inode: 88888,
        capture_timestamp_ms: None,
        gps_coords: None,
        trip_folder_name: "Bangalore".to_string(),
        original_stem: "photo_preserved_1".to_string(),
        relative_path: Some(PathBuf::from("Bangalore/photo_preserved_1.jpg")),
        resolved_location_name: None,
    };

    // Before metadata transfer, should_reclassify_bundle would trigger because None != Some
    assert!(
        should_reclassify_bundle(&scanned_bundle, &decision, &old_bundle, false),
        "Without metadata transfer, scanned bundle with None coordinates falsely triggers reclassification"
    );

    // Apply metadata transfer as performed in main.rs organize scan reconciliation
    if scanned_bundle.gps_coords.is_none() {
        scanned_bundle.gps_coords = old_bundle.gps_coords;
    }
    if scanned_bundle.capture_timestamp_ms.is_none() {
        scanned_bundle.capture_timestamp_ms = old_bundle.capture_timestamp_ms;
    }
    if scanned_bundle.resolved_location_name.is_none() {
        scanned_bundle.resolved_location_name = old_bundle.resolved_location_name.clone();
    }

    assert_eq!(scanned_bundle.gps_coords, cached_coords);
    assert_eq!(scanned_bundle.capture_timestamp_ms, cached_ts);
    assert_eq!(scanned_bundle.resolved_location_name, cached_loc_name);

    // After metadata transfer, should_reclassify_bundle must return false
    assert!(
        !should_reclassify_bundle(&scanned_bundle, &decision, &old_bundle, false),
        "With metadata transfer, cached bundle must NOT trigger reclassification on organize"
    );
}
