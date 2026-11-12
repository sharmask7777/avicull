use avicull::db::ManifestDb;
use avicull::types::{AssetBundle, AssetCategory, BundleDecision, Classification, RoutingTier};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_persist_and_retrieve_classification_db() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path)?;

    let run_id = "run_test_001";
    db.record_run(
        run_id,
        Path::new("/photos/trip"),
        Path::new("/photos/birds_hierarchy"),
        false,
        "{}",
    )?;

    let bundle1 = AssetBundle {
        bundle_id: "test_b1".to_string(),
        source_inode: 101,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: PathBuf::from("/photos/trip/DSC_0001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("trip/DSC_0001.JPG")),
    };
    let dec1 = BundleDecision {
        bundle_id: "test_b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Mammalia, "Snow_Leopard"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    db.record_bundles(run_id, &[dec1], &[bundle1])?;
    let all = db.get_all_bundles(None)?;
    assert_eq!(all.len(), 1);
    let (retrieved_dec, _) = &all[0];

    assert_eq!(retrieved_dec.category(), &AssetCategory::Mammalia);
    assert_eq!(retrieved_dec.entity_slug(), Some("Snow_Leopard"));
    assert_eq!(retrieved_dec.assigned_species_slug(), "Snow_Leopard");
    assert!(retrieved_dec.is_user_override);

    // Test 2: Top-level genre (Scenery with no species)
    let bundle2 = AssetBundle {
        bundle_id: "test_b2".to_string(),
        source_inode: 102,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0002".to_string(),
        primary_image: PathBuf::from("/photos/trip/DSC_0002.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("trip/DSC_0002.JPG")),
    };
    let dec2 = BundleDecision {
        bundle_id: "test_b2".to_string(),
        burst_id: None,
        detection_confidence: 0.5,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.5,
        final_confidence: 0.5,
        classification: Classification::top_level(AssetCategory::Scenery),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };
    db.record_bundles(run_id, &[dec2], &[bundle2])?;

    let all = db.get_all_bundles(None)?;
    assert_eq!(all.len(), 2);
    let b2_pair = all.iter().find(|(d, _)| d.bundle_id == "test_b2").unwrap();
    assert_eq!(b2_pair.0.category(), &AssetCategory::Scenery);
    assert_eq!(b2_pair.0.entity_slug(), None);
    assert_eq!(b2_pair.0.assigned_species_slug(), "");

    // Test 3: Override update via update_bundle_override_with_category
    db.update_bundle_override_with_category(
        "test_b1",
        "Reptilia",
        "Indian_Cobra",
        RoutingTier::ConfirmedSpecies,
        1.0,
    )?;

    let all_after = db.get_all_bundles(None)?;
    let b1_updated = all_after
        .iter()
        .find(|(d, _)| d.bundle_id == "test_b1")
        .unwrap();
    assert_eq!(b1_updated.0.category(), &AssetCategory::Reptilia);
    assert_eq!(b1_updated.0.entity_slug(), Some("Indian_Cobra"));
    assert!(b1_updated.0.is_user_override);

    // Test 4: Override update to top-level category with empty slug
    db.update_bundle_override_with_category(
        "test_b2",
        "Scenery",
        "",
        RoutingTier::ConfirmedSpecies,
        1.0,
    )?;

    let all_after2 = db.get_all_bundles(None)?;
    let b2_updated = all_after2
        .iter()
        .find(|(d, _)| d.bundle_id == "test_b2")
        .unwrap();
    assert_eq!(b2_updated.0.category(), &AssetCategory::Scenery);
    assert_eq!(b2_updated.0.entity_slug(), None);
    assert_eq!(b2_updated.0.assigned_species_slug(), "");
    assert!(b2_updated.0.is_user_override);

    // Test 5: ExistingBundleMap indexing contains classification
    let map = db.get_existing_bundle_map()?;
    let (cached_dec, _) = map.get("test_b1").unwrap();
    assert_eq!(cached_dec.category(), &AssetCategory::Reptilia);
    assert_eq!(cached_dec.entity_slug(), Some("Indian_Cobra"));

    Ok(())
}
