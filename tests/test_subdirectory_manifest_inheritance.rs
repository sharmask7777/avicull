use avicull::db::ManifestDb;
use avicull::types::{
    AssetBundle, BundleDecision, GpsCoordinate, LocationSource, RoutingTier, SpeciesPrediction,
};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn sample_bundle(subdir: &str, stem: &str) -> (BundleDecision, AssetBundle) {
    let bundle_id = format!("bundle_{}_{}", subdir, stem);
    let rel_path = PathBuf::from(format!("{}/{}.jpg", subdir, stem));
    let bundle = AssetBundle {
        bundle_id: bundle_id.clone(),
        source_inode: 12345,
        trip_folder_name: subdir.to_string(),
        original_stem: stem.to_string(),
        primary_image: PathBuf::from(format!("/fake/root/{}/{}.jpg", subdir, stem)),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: Some(1730000000000),
        gps_coords: Some(GpsCoordinate {
            latitude: 12.06,
            longitude: 76.15,
            source: LocationSource::SubdirectoryGazetteer,
        }),
        resolved_location_name: Some("Nagarhole".to_string()),
        relative_path: Some(rel_path),
    };

    let decision = BundleDecision {
        bundle_id,
        burst_id: None,
        detection_confidence: 0.99,
        bbox_xyxy_norm: Some([0.1, 0.1, 0.9, 0.9]),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "indpea1".into(),
            common_name: "Indian Peafowl".into(),
            folder_slug: "Indian_Peafowl".into(),
            scientific_name: "Pavo cristatus".into(),
            visual_prob: 0.96,
            geo_prior: 1.0,
            combined_score: 0.96,
        }],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Indian_Peafowl".into(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    (decision, bundle)
}

#[test]
fn test_discover_child_manifests_finds_subfolder_manifests() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create child trip A with in-place manifest
    let trip_a = root.join("TripA");
    let manifest_a = trip_a.join(".avicull/manifest.sqlite");
    std::fs::create_dir_all(manifest_a.parent().unwrap()).unwrap();
    ManifestDb::open_or_create(&manifest_a).unwrap();

    // Create child trip B with hierarchy manifest
    let trip_b = root.join("TripB");
    let manifest_b = trip_b.join("birds_hierarchy/.avicull/manifest.sqlite");
    std::fs::create_dir_all(manifest_b.parent().unwrap()).unwrap();
    ManifestDb::open_or_create(&manifest_b).unwrap();

    // Root output itself shouldn't be counted as child
    let root_out = root.join("birds_hierarchy/.avicull/manifest.sqlite");
    std::fs::create_dir_all(root_out.parent().unwrap()).unwrap();
    ManifestDb::open_or_create(&root_out).unwrap();

    let discovered = avicull::scanner::discover_child_manifests(root).unwrap();

    // Should discover manifest_a and manifest_b, but NOT root_out
    assert_eq!(
        discovered.len(),
        2,
        "Should discover exactly 2 child manifests"
    );
    let paths: Vec<PathBuf> = discovered.into_iter().map(|(p, _)| p).collect();
    let can_a = manifest_a.canonicalize().unwrap();
    let can_b = manifest_b.canonicalize().unwrap();
    assert!(paths.contains(&can_a));
    assert!(paths.contains(&can_b));
}

#[test]
fn test_import_child_manifest_merges_bundles_and_locations() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let child_dir = root.join("Nagarhole_2024");
    let child_db_path = child_dir.join(".avicull/manifest.sqlite");
    std::fs::create_dir_all(child_db_path.parent().unwrap()).unwrap();
    let child_db = ManifestDb::open_or_create(&child_db_path).unwrap();

    // Record a run and bundles in child db
    let (dec, bundle) = sample_bundle("Nagarhole_2024", "IMG_5001");
    child_db
        .record_run("child_run_1", &child_dir, &child_dir, false, "{}")
        .unwrap();
    child_db
        .record_bundle("child_run_1", &bundle, &dec)
        .unwrap();
    child_db.complete_run("child_run_1").unwrap();
    child_db
        .store_thumbnail(&bundle.bundle_id, "data:image/jpeg;base64,CHILD_THUMB")
        .unwrap();

    // Parent db
    let parent_db_path = root.join(".avicull/manifest.sqlite");
    std::fs::create_dir_all(parent_db_path.parent().unwrap()).unwrap();
    let parent_db = ManifestDb::open_or_create(&parent_db_path).unwrap();

    // Import child manifest
    let imported_count = parent_db
        .import_child_manifest(&child_db_path, Path::new("Nagarhole_2024"))
        .unwrap();

    assert_eq!(imported_count, 1, "Should have imported 1 bundle");

    // Verify parent existing_bundle_map has the imported bundle
    let existing_map = parent_db.get_existing_bundle_map().unwrap();
    assert!(
        existing_map.by_id.contains_key(&bundle.bundle_id),
        "Imported bundle must exist in parent ExistingBundleMap by ID"
    );

    // Verify thumbnail was also imported
    assert_eq!(
        parent_db.get_thumbnail_data_uri(&bundle.bundle_id).unwrap(),
        Some("data:image/jpeg;base64,CHILD_THUMB".to_string())
    );
}
