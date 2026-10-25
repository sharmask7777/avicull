use avicull::types::*;
use std::path::PathBuf;

#[test]
fn test_asset_bundle_serialization_roundtrip() {
    let bundle = AssetBundle {
        bundle_id: "test-bundle-123".to_string(),
        source_inode: 12345678,
        trip_folder_name: "2025-11_Bharatpur_Keoladeo".to_string(),
        original_stem: "DSC_4011".to_string(),
        primary_image: PathBuf::from("/photos/DSC_4011.ARW"),
        companion_images: vec![PathBuf::from("/photos/DSC_4011.JPG")],
        sidecar_files: vec![
            PathBuf::from("/photos/DSC_4011.xmp"),
            PathBuf::from("/photos/DSC_4011.ARW.xmp"),
        ],
        capture_timestamp_ms: Some(1732000000000),
        gps_coords: Some(GpsCoordinate {
            latitude: 27.1750,
            longitude: 77.5250,
            source: LocationSource::ExifGps,
        }),
        resolved_location_name: Some("Bharatpur".to_string()),
    };

    let serialized = serde_json::to_string(&bundle).expect("serialization failed");
    let deserialized: AssetBundle =
        serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(bundle, deserialized);

    // Verify backward compatibility: missing resolved_location_name in legacy JSON defaults to None
    let legacy_json = r#"{
        "bundle_id": "legacy-1",
        "source_inode": 100,
        "trip_folder_name": "Trip",
        "original_stem": "IMG_001",
        "primary_image": "/p/IMG_001.JPG",
        "companion_images": [],
        "sidecar_files": [],
        "capture_timestamp_ms": null,
        "gps_coords": null
    }"#;
    let legacy_bundle: AssetBundle = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(legacy_bundle.resolved_location_name, None);
}

#[test]
fn test_bundle_decision_serialization_roundtrip() {
    let prediction = SpeciesPrediction {
        taxon_id: "paisto1".to_string(),
        common_name: "Painted Stork".to_string(),
        folder_slug: "Painted_Stork".to_string(),
        scientific_name: "Mycteria leucocephala".to_string(),
        visual_prob: 0.95,
        geo_prior: 0.98,
        combined_score: 0.96,
    };

    let decision = BundleDecision {
        bundle_id: "test-bundle-123".to_string(),
        burst_id: Some("burst-001".to_string()),
        detection_confidence: 0.92,
        bbox_xyxy_norm: Some([0.1, 0.2, 0.8, 0.9]),
        top_predictions: vec![prediction],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
    };

    let serialized = serde_json::to_string(&decision).expect("serialization failed");
    let deserialized: BundleDecision =
        serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(decision, deserialized);
}

#[test]
fn test_organization_mode_default_and_parsing() {
    use clap::ValueEnum;

    // Verify Default is InPlaceFileRenameToDescriptiveNames
    assert_eq!(
        OrganizationMode::default(),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames
    );

    // Verify clap parsing from string for primary names
    assert_eq!(
        OrganizationMode::from_str("in-place", true).unwrap(),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames
    );
    assert_eq!(
        OrganizationMode::from_str("hierarchy", true).unwrap(),
        OrganizationMode::HierarchicalHardlinkHierarchy
    );

    // Verify clap parsing from aliases
    assert_eq!(
        OrganizationMode::from_str("in-place-file-rename-to-descriptive-names", true).unwrap(),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames
    );
    assert_eq!(
        OrganizationMode::from_str("hierarchical-hardlink-hierarchy", true).unwrap(),
        OrganizationMode::HierarchicalHardlinkHierarchy
    );
    assert_eq!(
        OrganizationMode::from_str("hardlink", true).unwrap(),
        OrganizationMode::HierarchicalHardlinkHierarchy
    );

    // Verify invalid values fail
    assert!(OrganizationMode::from_str("invalid-mode", true).is_err());
}

#[test]
fn test_organization_mode_serialization_roundtrip() {
    let mode_inplace = OrganizationMode::InPlaceFileRenameToDescriptiveNames;
    let serialized_inplace = serde_json::to_string(&mode_inplace).expect("serialization failed");
    let deserialized_inplace: OrganizationMode =
        serde_json::from_str(&serialized_inplace).expect("deserialization failed");
    assert_eq!(mode_inplace, deserialized_inplace);

    let mode_hierarchy = OrganizationMode::HierarchicalHardlinkHierarchy;
    let serialized_hierarchy =
        serde_json::to_string(&mode_hierarchy).expect("serialization failed");
    let deserialized_hierarchy: OrganizationMode =
        serde_json::from_str(&serialized_hierarchy).expect("deserialization failed");
    assert_eq!(mode_hierarchy, deserialized_hierarchy);
}
