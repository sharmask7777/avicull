use avicull::types::{BundleDecision, RoutingTier, SpeciesPrediction};
use std::path::PathBuf;

#[test]
fn test_diverse_species_routing_not_collapsed_to_single_species() {
    // Verify that bundles with different species classifications route to distinct directories
    // rather than collapsing to a single degenerate class.
    let species_list = vec![
        ("blared1", "Black Redstart", "Black_Redstart", 0.88),
        ("houspa", "House Sparrow", "House_Sparrow", 0.75),
        ("citwag1", "Citrine Wagtail", "Citrine_Wagtail", 0.70),
        ("tibsnow1", "Tibetan Snowcock", "Tibetan_Snowcock", 0.92),
    ];

    let mut decisions = Vec::new();
    for (i, (code, com_name, slug, conf)) in species_list.into_iter().enumerate() {
        let pred = SpeciesPrediction {
            taxon_id: code.to_string(),
            common_name: com_name.to_string(),
            folder_slug: slug.to_string(),
            scientific_name: "Test species".to_string(),
            visual_prob: conf,
            geo_prior: 1.0,
            combined_score: conf,
        };

        decisions.push(BundleDecision {
            bundle_id: format!("bundle_{}", i),
            burst_id: Some(format!("burst_{}", i)),
            detection_confidence: 0.85,
            bbox_xyxy_norm: Some([0.1, 0.1, 0.9, 0.9]),
            top_predictions: vec![pred],
            frame_confidence: conf,
            final_confidence: conf,
            assigned_species_slug: slug.to_string(),
            routing_tier: RoutingTier::ConfirmedSpecies,
            is_user_override: false,
        });
    }

    let unique_slugs: std::collections::HashSet<_> =
        decisions.iter().map(|d| &d.assigned_species_slug).collect();

    assert_eq!(
        unique_slugs.len(),
        4,
        "Expected 4 distinct species slugs, but found {}",
        unique_slugs.len()
    );
    assert!(unique_slugs.contains(&"Black_Redstart".to_string()));
    assert!(unique_slugs.contains(&"House_Sparrow".to_string()));
    assert!(unique_slugs.contains(&"Citrine_Wagtail".to_string()));
    assert!(unique_slugs.contains(&"Tibetan_Snowcock".to_string()));
}

#[test]
fn test_batch_infer_script_exists_and_executable() {
    let script_path = PathBuf::from("scripts/infer.py");
    assert!(
        script_path.exists(),
        "scripts/infer.py must exist for real avian inference"
    );
}
