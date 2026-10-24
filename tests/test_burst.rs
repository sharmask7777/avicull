use avicull::burst::*;
use avicull::types::*;

#[test]
fn test_burst_consensus_smoothing_5_frames() {
    // 5 frames taken 200 ms apart:
    // Frames 0, 1, 2, 4 predict White-throated Kingfisher at 0.92
    // Frame 3 (motion blurred) predicts Collared Kingfisher at 0.51
    // Cosine similarity between all 5 frames is high (0.88+)

    let kingfisher_pred = SpeciesPrediction {
        taxon_id: "whbkin1".to_string(),
        common_name: "White-throated Kingfisher".to_string(),
        folder_slug: "White-throated_Kingfisher".to_string(),
        scientific_name: "Halcyon smyrnensis".to_string(),
        visual_prob: 0.92,
        geo_prior: 1.0,
        combined_score: 0.92,
    };

    let blurred_pred = SpeciesPrediction {
        taxon_id: "colkin1".to_string(),
        common_name: "Collared Kingfisher".to_string(),
        folder_slug: "Collared_Kingfisher".to_string(),
        scientific_name: "Todiramphus chloris".to_string(),
        visual_prob: 0.51,
        geo_prior: 1.0,
        combined_score: 0.51,
    };

    let mut items = Vec::new();
    let embedding = vec![1.0; 512]; // unit embedding for cosine similarity = 1.0

    for i in 0..5 {
        let ts = 1000 + i * 200;
        let pred = if i == 3 {
            blurred_pred.clone()
        } else {
            kingfisher_pred.clone()
        };
        items.push(ClassifiedBundleItem {
            bundle_id: format!("b{}", i),
            trip_folder: "Trip1".to_string(),
            capture_timestamp_ms: Some(ts),
            detection_confidence: if i == 3 { 0.40 } else { 0.95 },
            bbox_norm: Some([0.1, 0.1, 0.8, 0.8]),
            embedding: embedding.clone(),
            top_predictions: vec![pred],
            posteriors: if i == 3 {
                vec![("colkin1".to_string(), 0.51), ("whbkin1".to_string(), 0.45)]
            } else {
                vec![("whbkin1".to_string(), 0.92), ("colkin1".to_string(), 0.05)]
            },
        });
    }

    let decisions =
        smooth_burst_consensus(&items, 3000, 0.80, ConfidenceMode::BurstConsensus, 0.65);
    assert_eq!(decisions.len(), 5);

    // Assert ALL 5 frames receive assigned_species_slug == "White-throated_Kingfisher"
    // and RoutingTier::ConfirmedSpecies
    for (idx, d) in decisions.iter().enumerate() {
        assert_eq!(
            d.assigned_species_slug, "White-throated_Kingfisher",
            "Frame {} must be smoothed to White-throated Kingfisher",
            idx
        );
        assert_eq!(
            d.routing_tier,
            RoutingTier::ConfirmedSpecies,
            "Frame {} must be ConfirmedSpecies",
            idx
        );
        assert!(d.final_confidence >= 0.65);
    }
}

#[test]
fn test_burst_split_when_camera_pans_to_different_bird() {
    let mut items = Vec::new();

    // Frame 0: Bird A with embedding [1, 0, 0, ...]
    let mut emb_a = vec![0.0f32; 512];
    emb_a[0] = 1.0;
    items.push(ClassifiedBundleItem {
        bundle_id: "b0".to_string(),
        trip_folder: "Trip1".to_string(),
        capture_timestamp_ms: Some(1000),
        detection_confidence: 0.9,
        bbox_norm: None,
        embedding: emb_a,
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "paisto1".to_string(),
            common_name: "Painted Stork".to_string(),
            folder_slug: "Painted_Stork".to_string(),
            scientific_name: "Mycteria leucocephala".to_string(),
            visual_prob: 0.9,
            geo_prior: 1.0,
            combined_score: 0.9,
        }],
        posteriors: vec![("paisto1".to_string(), 0.9)],
    });

    // Frame 1: 500 ms later, but camera panned to Bird B with orthogonal embedding [0, 1, 0, ...]
    // Cosine similarity = 0.0 (< 0.80) -> MUST SPLIT BURST!
    let mut emb_b = vec![0.0f32; 512];
    emb_b[1] = 1.0;
    items.push(ClassifiedBundleItem {
        bundle_id: "b1".to_string(),
        trip_folder: "Trip1".to_string(),
        capture_timestamp_ms: Some(1500),
        detection_confidence: 0.9,
        bbox_norm: None,
        embedding: emb_b,
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "sttswa1".to_string(),
            common_name: "Streak-throated Swallow".to_string(),
            folder_slug: "Streak-throated_Swallow".to_string(),
            scientific_name: "Petrochelidon fluvicola".to_string(),
            visual_prob: 0.9,
            geo_prior: 1.0,
            combined_score: 0.9,
        }],
        posteriors: vec![("sttswa1".to_string(), 0.9)],
    });

    let decisions =
        smooth_burst_consensus(&items, 3000, 0.80, ConfidenceMode::BurstConsensus, 0.65);
    assert_eq!(decisions.len(), 2);
    assert_ne!(
        decisions[0].burst_id, decisions[1].burst_id,
        "Low visual similarity must split burst"
    );
    assert_eq!(decisions[0].assigned_species_slug, "Painted_Stork");
    assert_eq!(
        decisions[1].assigned_species_slug,
        "Streak-throated_Swallow"
    );
}
