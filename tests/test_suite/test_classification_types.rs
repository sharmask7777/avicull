use avicull::types::{AssetCategory, BundleDecision, Classification, RoutingTier};

#[test]
fn test_classification_top_level_and_granular() {
    let top_mammal = Classification::top_level(AssetCategory::Mammalia);
    assert_eq!(top_mammal.category, AssetCategory::Mammalia);
    assert_eq!(top_mammal.entity_slug, None);

    let granular_mammal = Classification::granular(AssetCategory::Mammalia, "Snow_Leopard");
    assert_eq!(granular_mammal.category, AssetCategory::Mammalia);
    assert_eq!(granular_mammal.entity_slug.as_deref(), Some("Snow_Leopard"));

    let empty_granular = Classification::granular(AssetCategory::Scenery, "   ");
    assert_eq!(empty_granular.entity_slug, None);
}

#[test]
fn test_bundle_decision_classification_accessors() {
    let dec = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        classification: Classification::granular(AssetCategory::Mammalia, "Snow_Leopard"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    assert_eq!(dec.category(), &AssetCategory::Mammalia);
    assert_eq!(dec.entity_slug(), Some("Snow_Leopard"));
    assert_eq!(dec.assigned_species_slug(), "Snow_Leopard");
    assert_eq!(
        dec.destination_path_tokens(),
        ("Mammalia", Some("Snow_Leopard"))
    );

    let top_dec = BundleDecision {
        classification: Classification::top_level(AssetCategory::Scenery),
        ..dec
    };
    assert_eq!(top_dec.category(), &AssetCategory::Scenery);
    assert_eq!(top_dec.entity_slug(), None);
    assert_eq!(top_dec.assigned_species_slug(), "");
    assert_eq!(top_dec.destination_path_tokens(), ("Scenery", None));
}
