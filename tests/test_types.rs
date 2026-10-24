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
    };

    let serialized = serde_json::to_string(&bundle).expect("serialization failed");
    let deserialized: AssetBundle =
        serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(bundle, deserialized);
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
