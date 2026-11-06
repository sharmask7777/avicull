//! # Extensible Multi-Taxa Classification & Architectural Guardrail Tests
//!
//! Enforces the architectural invariant that classification, burst consensus, and directory
//! routing are domain-agnostic and support non-avian categories (`Mammalia`, `Insecta`,
//! `Scenery`, `Portrait`) alongside avian taxa.
//!
//! These tests serve as a permanent regression guardrail ensuring that future refactors
//! cannot restrict classification models or burst clustering back to bird-only types.

use avicull::burst::{burst_consensus_multi_taxa, ClassifiedBundleItem};
use avicull::classifier::{
    MockSpeciesClassifier, MockTaxonClassifier, SpeciesClassifier, TaxonClassifier,
};
use avicull::types::{
    AssetCategory, ConfidenceMode, GpsCoordinate, LocationSource, RoutingTier, SpeciesPrediction,
};
use image::DynamicImage;

#[test]
fn test_taxon_classifier_extensibility_non_avian_domains() {
    let mammal_pred = SpeciesPrediction {
        taxon_id: "malabs1".to_string(),
        common_name: "Malabar Giant Squirrel".to_string(),
        folder_slug: "Mammalia_malabar_giant_squirrel".to_string(),
        scientific_name: "Ratufa indica".to_string(),
        visual_prob: 0.94,
        geo_prior: 1.0,
        combined_score: 0.94,
    };

    let insect_pred = SpeciesPrediction {
        taxon_id: "commim1".to_string(),
        common_name: "Common Mime".to_string(),
        folder_slug: "Insecta_common_mime".to_string(),
        scientific_name: "Papilio clytia".to_string(),
        visual_prob: 0.89,
        geo_prior: 1.0,
        combined_score: 0.89,
    };

    let scenery_pred = SpeciesPrediction {
        taxon_id: "scen_wg1".to_string(),
        common_name: "Western Ghats Rainforest Canopy".to_string(),
        folder_slug: "Scenery_western_ghats".to_string(),
        scientific_name: "Landscape".to_string(),
        visual_prob: 0.99,
        geo_prior: 1.0,
        combined_score: 0.99,
    };

    let portrait_pred = SpeciesPrediction {
        taxon_id: "port_ph1".to_string(),
        common_name: "Wildlife Photographer in Action".to_string(),
        folder_slug: "Portrait_photographer".to_string(),
        scientific_name: "Homo sapiens".to_string(),
        visual_prob: 0.92,
        geo_prior: 1.0,
        combined_score: 0.92,
    };

    let dummy_img = DynamicImage::new_rgb8(224, 224);
    let coords = Some(GpsCoordinate {
        latitude: 11.41,
        longitude: 76.70,
        source: LocationSource::ExifGps,
    });

    // Test Mammalia classification via TaxonClassifier trait
    let mammal_classifier = MockTaxonClassifier::new(vec![mammal_pred]);
    let (emb_m, preds_m) = mammal_classifier
        .classify(&dummy_img, coords, 0.5)
        .expect("Mammalia classification must succeed");
    assert_eq!(emb_m.len(), 512, "Visual embedding must be 512-dim");
    assert_eq!(preds_m.len(), 1);
    assert_eq!(
        AssetCategory::from_slug(&preds_m[0].folder_slug),
        AssetCategory::Mammalia
    );

    // Test Insecta classification with regional checklist (non-avian decoupling check)
    let insect_classifier = MockTaxonClassifier::new(vec![insect_pred]);
    let (emb_i, preds_i) = insect_classifier
        .classify_with_region(&dummy_img, coords, Some("IN-KL"), 0.5)
        .expect("Insecta classification must succeed");
    assert_eq!(emb_i.len(), 512);
    assert_eq!(
        AssetCategory::from_slug(&preds_i[0].folder_slug),
        AssetCategory::Insecta
    );
    // Non-avian taxon should not be penalized by avian checklist
    assert!(
        preds_i[0].combined_score >= 0.80,
        "Non-avian taxon must not be zeroed/attenuated by avian checklist: score = {}",
        preds_i[0].combined_score
    );

    // Test Scenery classification
    let scenery_classifier = MockTaxonClassifier::new(vec![scenery_pred]);
    let (_emb_s, preds_s) = scenery_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Scenery classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_s[0].folder_slug),
        AssetCategory::Scenery
    );

    // Test Reptilia classification
    let reptile_pred = SpeciesPrediction {
        taxon_id: "rept_mugger1".to_string(),
        common_name: "Mugger Crocodile".to_string(),
        folder_slug: "Reptilia/Mugger_Crocodile".to_string(),
        scientific_name: "Crocodylus palustris".to_string(),
        visual_prob: 0.94,
        geo_prior: 1.0,
        combined_score: 0.94,
    };
    let reptile_classifier = MockTaxonClassifier::new(vec![reptile_pred]);
    let (_emb_r, preds_r) = reptile_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Reptilia classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_r[0].folder_slug),
        AssetCategory::Reptilia
    );

    // Test Amphibia classification
    let amphibian_pred = SpeciesPrediction {
        taxon_id: "amph_bullfrog1".to_string(),
        common_name: "American Bullfrog".to_string(),
        folder_slug: "Amphibia/American_Bullfrog".to_string(),
        scientific_name: "Lithobates catesbeianus".to_string(),
        visual_prob: 0.91,
        geo_prior: 1.0,
        combined_score: 0.91,
    };
    let amphibian_classifier = MockTaxonClassifier::new(vec![amphibian_pred]);
    let (_emb_a, preds_a) = amphibian_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Amphibia classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_a[0].folder_slug),
        AssetCategory::Amphibia
    );

    // Test Portrait (Other) classification
    let portrait_classifier = MockTaxonClassifier::new(vec![portrait_pred]);
    let (_emb_p, preds_p) = portrait_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Portrait classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_p[0].folder_slug),
        AssetCategory::Other("Portrait".to_string())
    );
}

#[test]
fn test_backward_compatibility_species_classifier_alias() {
    let pred = SpeciesPrediction {
        taxon_id: "whbkin1".to_string(),
        common_name: "White-throated Kingfisher".to_string(),
        folder_slug: "White-throated_Kingfisher".to_string(),
        scientific_name: "Halcyon smyrnensis".to_string(),
        visual_prob: 0.95,
        geo_prior: 1.0,
        combined_score: 0.95,
    };

    let classifier: Box<dyn SpeciesClassifier> = Box::new(MockSpeciesClassifier::new(vec![pred]));
    let dummy_img = DynamicImage::new_rgb8(224, 224);
    let (emb, preds) = classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Classify must succeed via SpeciesClassifier trait object");
    assert_eq!(emb.len(), 512);
    assert_eq!(preds[0].taxon_id, "whbkin1");

    // Also assert Box<dyn TaxonClassifier> works with MockSpeciesClassifier
    let taxon_box: Box<dyn TaxonClassifier> = classifier;
    let (emb2, _) = taxon_box
        .classify(&dummy_img, None, 0.0)
        .expect("Classify must succeed via TaxonClassifier trait object");
    assert_eq!(emb2.len(), 512);
}

#[test]
fn test_burst_clustering_operations_on_arbitrary_category_items() {
    let mut items = Vec::new();

    // 2 Mammalia frames close together in time (< 1 sec)
    for i in 0..2 {
        let mut emb = vec![0.0f32; 512];
        emb[0] = 1.0;
        items.push(ClassifiedBundleItem {
            bundle_id: format!("mammal_{i}"),
            trip_folder: "Wayanad".to_string(),
            capture_timestamp_ms: Some(1000 + i * 200),
            detection_confidence: 0.95,
            bbox_norm: Some([0.1, 0.1, 0.8, 0.8]),
            embedding: emb,
            top_predictions: vec![SpeciesPrediction {
                taxon_id: "malabs1".to_string(),
                common_name: "Malabar Giant Squirrel".to_string(),
                folder_slug: "Mammalia_malabar_giant_squirrel".to_string(),
                scientific_name: "Ratufa indica".to_string(),
                visual_prob: 0.92,
                geo_prior: 1.0,
                combined_score: 0.92,
            }],
            posteriors: vec![("malabs1".to_string(), 0.92)],
        });
    }

    // 1 Insecta frame taken 300 ms later (same trip, but different category)
    {
        let mut emb = vec![0.0f32; 512];
        emb[1] = 1.0;
        items.push(ClassifiedBundleItem {
            bundle_id: "insect_0".to_string(),
            trip_folder: "Wayanad".to_string(),
            capture_timestamp_ms: Some(1500),
            detection_confidence: 0.90,
            bbox_norm: Some([0.2, 0.2, 0.7, 0.7]),
            embedding: emb,
            top_predictions: vec![SpeciesPrediction {
                taxon_id: "commim1".to_string(),
                common_name: "Common Mime".to_string(),
                folder_slug: "Insecta_common_mime".to_string(),
                scientific_name: "Papilio clytia".to_string(),
                visual_prob: 0.88,
                geo_prior: 1.0,
                combined_score: 0.88,
            }],
            posteriors: vec![("commim1".to_string(), 0.88)],
        });
    }

    // 2 Scenery frames
    for i in 0..2 {
        let mut emb = vec![0.0f32; 512];
        emb[2] = 1.0;
        items.push(ClassifiedBundleItem {
            bundle_id: format!("scenery_{i}"),
            trip_folder: "Wayanad".to_string(),
            capture_timestamp_ms: Some(3000 + i * 100),
            detection_confidence: 0.85,
            bbox_norm: None,
            embedding: emb,
            top_predictions: vec![SpeciesPrediction {
                taxon_id: "scen1".to_string(),
                common_name: "Western Ghats Vista".to_string(),
                folder_slug: "Scenery_western_ghats".to_string(),
                scientific_name: "Landscape".to_string(),
                visual_prob: 0.99,
                geo_prior: 1.0,
                combined_score: 0.99,
            }],
            posteriors: vec![("scen1".to_string(), 0.99)],
        });
    }

    let decisions = burst_consensus_multi_taxa(&items, ConfidenceMode::BurstConsensus, 0.65);
    assert_eq!(decisions.len(), 5, "Every item must have a decision");

    // Assert embeddings are preserved on original items
    for item in &items {
        assert_eq!(
            item.embedding.len(),
            512,
            "Item {} must preserve 512-dim embedding",
            item.bundle_id
        );
        let sum_sq: f32 = item.embedding.iter().map(|x| x * x).sum();
        assert!(
            (sum_sq - 1.0).abs() < 1e-4,
            "Item {} embedding magnitude must be preserved",
            item.bundle_id
        );
    }

    // Mammalia frames must be in the same burst
    assert_eq!(
        decisions[0].assigned_species_slug,
        "Mammalia_malabar_giant_squirrel"
    );
    assert_eq!(
        decisions[1].assigned_species_slug,
        "Mammalia_malabar_giant_squirrel"
    );
    assert_eq!(decisions[0].routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(decisions[1].routing_tier, RoutingTier::ConfirmedSpecies);
    assert_eq!(decisions[0].burst_id, decisions[1].burst_id);

    // Insect frame must be in a DIFFERENT burst despite being close in time (category partitioning)
    assert_eq!(decisions[2].assigned_species_slug, "Insecta_common_mime");
    assert_ne!(
        decisions[2].burst_id, decisions[0].burst_id,
        "Insecta item must not merge into Mammalia burst"
    );

    // Scenery frames must be in their own burst
    assert_eq!(decisions[3].assigned_species_slug, "Scenery_western_ghats");
    assert_eq!(decisions[4].assigned_species_slug, "Scenery_western_ghats");
    assert_eq!(decisions[3].burst_id, decisions[4].burst_id);
    assert_ne!(decisions[3].burst_id, decisions[2].burst_id);
}

#[test]
fn test_non_confident_items_route_to_no_subject_detected_alias() {
    let mut emb = vec![0.0f32; 512];
    emb[0] = 0.5;

    let low_detection_item = ClassifiedBundleItem {
        bundle_id: "low_det_mammal".to_string(),
        trip_folder: "Wayanad".to_string(),
        capture_timestamp_ms: Some(1000),
        detection_confidence: 0.10, // Below 0.25 min_detection threshold
        bbox_norm: None,
        embedding: emb.clone(),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "malabs1".to_string(),
            common_name: "Malabar Giant Squirrel".to_string(),
            folder_slug: "Mammalia_malabar_giant_squirrel".to_string(),
            scientific_name: "Ratufa indica".to_string(),
            visual_prob: 0.40,
            geo_prior: 1.0,
            combined_score: 0.40,
        }],
        posteriors: vec![("malabs1".to_string(), 0.40)],
    };

    let empty_prediction_item = ClassifiedBundleItem {
        bundle_id: "empty_pred_blur".to_string(),
        trip_folder: "Wayanad".to_string(),
        capture_timestamp_ms: Some(1200),
        detection_confidence: 0.0,
        bbox_norm: None,
        embedding: emb,
        top_predictions: vec![],
        posteriors: vec![],
    };

    let decisions = burst_consensus_multi_taxa(
        &[low_detection_item, empty_prediction_item],
        ConfidenceMode::BurstConsensus,
        0.65,
    );

    assert_eq!(decisions.len(), 2);

    // Assert that NoSubjectDetected alias works and equals NoBirdDetected
    assert_eq!(RoutingTier::NoSubjectDetected, RoutingTier::NoBirdDetected);

    assert_eq!(
        decisions[0].routing_tier,
        RoutingTier::NoSubjectDetected,
        "Low detection mammal must route to NoSubjectDetected / NoBirdDetected"
    );
    assert_eq!(
        decisions[0].routing_tier,
        RoutingTier::NoBirdDetected,
        "Must also match RoutingTier::NoBirdDetected"
    );

    assert_eq!(
        decisions[1].routing_tier,
        RoutingTier::NoSubjectDetected,
        "Zero detection blurred frame must route to NoSubjectDetected"
    );

    // Verify AssetCategory::from_slug on unclassified / no-subject slugs
    assert_eq!(
        AssetCategory::from_slug("_No_Bird_Detected"),
        AssetCategory::NoSubjectDetected
    );
    assert_eq!(
        AssetCategory::from_slug("_No_Subject_Detected"),
        AssetCategory::NoSubjectDetected
    );
    assert_eq!(
        AssetCategory::from_slug("No_Bird_Detected"),
        AssetCategory::NoSubjectDetected
    );
    assert_eq!(
        AssetCategory::from_slug("No_Subject_Detected"),
        AssetCategory::NoSubjectDetected
    );

    // Verify resolve_category_for_decision resolves to NoSubjectDetected
    assert_eq!(
        avicull::organizer::resolve_category_for_decision(&decisions[0]),
        AssetCategory::NoSubjectDetected
    );
    assert_eq!(
        avicull::organizer::resolve_category_for_decision(&decisions[1]),
        AssetCategory::NoSubjectDetected
    );
}
