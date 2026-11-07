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

    // Test Architecture classification
    let architecture_pred = SpeciesPrediction {
        taxon_id: "arch_taj_mahal1".to_string(),
        common_name: "Taj Mahal".to_string(),
        folder_slug: "Architecture/Taj_Mahal".to_string(),
        scientific_name: "Monumentum Taj Mahal".to_string(),
        visual_prob: 0.96,
        geo_prior: 1.0,
        combined_score: 0.96,
    };
    let architecture_classifier = MockTaxonClassifier::new(vec![architecture_pred]);
    let (_emb_arch, preds_arch) = architecture_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Architecture classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_arch[0].folder_slug),
        AssetCategory::Architecture
    );

    // Test Portrait classification
    let portrait_classifier = MockTaxonClassifier::new(vec![portrait_pred]);
    let (_emb_p, preds_p) = portrait_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Portrait classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_p[0].folder_slug),
        AssetCategory::Portrait
    );

    // Test Abstract classification
    let abstract_pred = SpeciesPrediction {
        taxon_id: "abst_minimal_wave1".to_string(),
        common_name: "Water Ripples Pattern".to_string(),
        folder_slug: "Abstract/Water_Ripples_Pattern".to_string(),
        scientific_name: "Pattern Abstract".to_string(),
        visual_prob: 0.89,
        geo_prior: 1.0,
        combined_score: 0.89,
    };
    let abstract_classifier = MockTaxonClassifier::new(vec![abstract_pred]);
    let (_emb_abs, preds_abs) = abstract_classifier
        .classify(&dummy_img, None, 0.0)
        .expect("Abstract classification must succeed");
    assert_eq!(
        AssetCategory::from_slug(&preds_abs[0].folder_slug),
        AssetCategory::Abstract
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

#[test]
fn test_curated_species_slug_without_prefix_resolves_category() {
    // 1. Slugs without "Mammalia_" prefix must resolve to Mammalia directly via curated registry
    assert_eq!(
        AssetCategory::from_slug("Snow_Leopard"),
        AssetCategory::Mammalia
    );
    assert_eq!(
        AssetCategory::from_slug("Bengal_Tiger"),
        AssetCategory::Mammalia
    );
    assert_eq!(
        AssetCategory::from_slug("Asian_Elephant"),
        AssetCategory::Mammalia
    );

    // 2. Insecta without prefix
    assert_eq!(
        AssetCategory::from_slug("Monarch_Butterfly"),
        AssetCategory::Insecta
    );

    // 3. Amphibia without prefix
    assert_eq!(
        AssetCategory::from_slug("Common_Indian_Toad"),
        AssetCategory::Amphibia
    );

    // 4. Architecture without prefix
    assert_eq!(
        AssetCategory::from_slug("Monumental_Architecture"),
        AssetCategory::Architecture
    );

    // 5. Portrait without prefix
    assert_eq!(
        AssetCategory::from_slug("Cultural_Street_Portrait"),
        AssetCategory::Portrait
    );

    // 6. SpeciesPrediction method .category()
    let snow_leopard_pred = SpeciesPrediction {
        taxon_id: "mamm_snow_leop1".to_string(),
        common_name: "Snow Leopard".to_string(),
        folder_slug: "Snow_Leopard".to_string(),
        scientific_name: "Panthera uncia".to_string(),
        visual_prob: 0.96,
        geo_prior: 1.0,
        combined_score: 0.96,
    };
    assert_eq!(snow_leopard_pred.category(), AssetCategory::Mammalia);

    // 7. ClassifiedBundleItem category resolution in burst consensus
    let item = ClassifiedBundleItem {
        bundle_id: "snow_leop_burst_1".to_string(),
        trip_folder: "Ladakh".to_string(),
        capture_timestamp_ms: Some(2000),
        detection_confidence: 0.95,
        bbox_norm: None,
        embedding: vec![0.0; 512],
        top_predictions: vec![snow_leopard_pred],
        posteriors: vec![("mamm_snow_leop1".to_string(), 0.96)],
    };
    assert_eq!(item.category(), AssetCategory::Mammalia);
}

#[test]
fn test_guardrail_zero_personal_context_leaks_in_source_code() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let scripts_dir = manifest_dir.join("scripts");

    let mut violations = Vec::new();

    let check_dir = |dir: &std::path::Path, violations: &mut Vec<String>| {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "rs" && ext != "py" {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                // Permit illustrative comments in path_parser.rs explaining stripped prefix patterns
                if path.ends_with("src/location/path_parser.rs")
                    && (trimmed.starts_with("//") || trimmed.contains("strip"))
                {
                    continue;
                }

                // Deny hardcoded personal paths
                if line.contains("/Users/") || line.contains("/home/") {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(manifest_dir).unwrap_or(path).display(),
                        line_num + 1,
                        trimmed
                    ));
                }
            }
        }
    };

    check_dir(&src_dir, &mut violations);
    check_dir(&scripts_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "Detected personal context leaks in production source code:\n{}",
        violations.join("\n")
    );
}

#[test]
fn test_guardrail_zero_procedural_agent_meta_artifacts() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let scripts_dir = manifest_dir.join("scripts");

    let patterns = [
        "// step ",
        "// phase ",
        "# step ",
        "# phase ",
        "// step:",
        "// phase:",
    ];
    let mut violations = Vec::new();

    let check_dir = |dir: &std::path::Path, violations: &mut Vec<String>| {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "rs" && ext != "py" {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (line_num, line) in content.lines().enumerate() {
                let lower = line.to_lowercase();
                for pat in &patterns {
                    if lower.contains(pat) {
                        violations.push(format!(
                            "{}:{}: {}",
                            path.strip_prefix(manifest_dir).unwrap_or(path).display(),
                            line_num + 1,
                            line.trim()
                        ));
                        break;
                    }
                }
            }
        }
    };

    check_dir(&src_dir, &mut violations);
    check_dir(&scripts_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "Detected procedural agent meta-artifacts (// Step X) in production source code:\n{}",
        violations.join("\n")
    );
}

#[test]
fn test_guardrail_curated_multi_taxa_full_dataset_coverage() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let json_path = manifest_dir.join("data/curated_multi_taxa.json");
    let content = std::fs::read_to_string(json_path).expect("curated_multi_taxa.json must exist");

    #[derive(serde::Deserialize)]
    struct Entry {
        taxon_id: String,
        category: String,
        common_name: String,
        #[allow(dead_code)]
        scientific_name: String,
        aliases: Option<Vec<String>>,
    }

    let entries: Vec<Entry> = serde_json::from_str(&content).expect("Valid JSON");
    assert!(
        entries.len() >= 200,
        "Curated multi-taxa dataset must contain at least 200 entries, found {}",
        entries.len()
    );

    for entry in &entries {
        let expected_cat: AssetCategory = entry.category.parse().unwrap();
        assert_ne!(
            expected_cat,
            AssetCategory::Bird,
            "Curated entry {} should be non-avian",
            entry.common_name
        );

        // Verify folder_slug derived from common name
        let folder_slug = entry.common_name.replace(' ', "_");
        let resolved_by_slug = AssetCategory::from_slug(&folder_slug);
        assert_eq!(
            resolved_by_slug, expected_cat,
            "folder_slug '{}' must resolve to {:?}",
            folder_slug, expected_cat
        );

        // Verify common_name resolution
        let resolved_by_name = AssetCategory::from_slug(&entry.common_name);
        assert_eq!(
            resolved_by_name, expected_cat,
            "common_name '{}' must resolve to {:?}",
            entry.common_name, expected_cat
        );

        // Verify taxon_id resolution
        let resolved_by_id = AssetCategory::from_slug(&entry.taxon_id);
        assert_eq!(
            resolved_by_id, expected_cat,
            "taxon_id '{}' must resolve to {:?}",
            entry.taxon_id, expected_cat
        );

        // Verify aliases resolution
        if let Some(ref aliases) = entry.aliases {
            for alias in aliases {
                let resolved_alias = AssetCategory::from_slug(alias);
                assert_eq!(
                    resolved_alias, expected_cat,
                    "alias '{}' must resolve to {:?}",
                    alias, expected_cat
                );
            }
        }
    }
}

#[test]
fn test_guardrail_never_fallback_to_bird_on_unknown_or_botanical_slugs() {
    // 1. AssetCategory::default() MUST NOT be Bird
    assert_ne!(
        AssetCategory::default(),
        AssetCategory::Bird,
        "AssetCategory::default() must never be Bird; should be NoSubjectDetected"
    );
    assert_eq!(
        AssetCategory::default(),
        AssetCategory::NoSubjectDetected,
        "AssetCategory::default() must be NoSubjectDetected"
    );

    // 2. Unknown or legacy botanical slugs MUST NOT resolve to Bird
    assert_ne!(
        AssetCategory::from_slug("Banyan_Tree"),
        AssetCategory::Bird,
        "Banyan_Tree must never resolve to Bird"
    );
    assert_ne!(
        AssetCategory::from_slug("Sacred_Lotus"),
        AssetCategory::Bird,
        "Sacred_Lotus must never resolve to Bird"
    );
    assert_ne!(
        AssetCategory::from_slug("completely_random_nonexistent_noun"),
        AssetCategory::Bird,
        "Unknown slug must never resolve to Bird"
    );
    assert_ne!(
        AssetCategory::from_slug("Unknown"),
        AssetCategory::Bird,
        "Unknown sentinel must never resolve to Bird"
    );

    // 3. Known non-avian categories must resolve properly
    assert_eq!(
        AssetCategory::from_slug("Desert_Dunes"),
        AssetCategory::Scenery
    );
    assert_eq!(
        AssetCategory::from_slug("Close-up_Human_Portrait"),
        AssetCategory::Portrait
    );
    assert_eq!(
        AssetCategory::from_slug("Historic_Fort_and_Palace"),
        AssetCategory::Architecture
    );

    // 4. Zero detection sentinels
    assert_eq!(
        AssetCategory::from_slug("_No_Bird_Detected"),
        AssetCategory::NoSubjectDetected
    );
    assert_eq!(
        AssetCategory::from_slug("_No_Subject_Detected"),
        AssetCategory::NoSubjectDetected
    );
}

#[test]
fn test_guardrail_sqlite_schema_no_default_bird() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let db_src =
        std::fs::read_to_string(manifest_dir.join("src/db.rs")).expect("src/db.rs must exist");

    // Guardrail: No SQLite table definition or ALTER TABLE in src/db.rs should default category to 'Bird'
    for (line_idx, line) in db_src.lines().enumerate() {
        let upper = line.to_uppercase();
        if upper.contains("DEFAULT 'BIRD'") || upper.contains("DEFAULT \"BIRD\"") {
            panic!(
                "src/db.rs line {} violates architectural invariant by specifying DEFAULT 'Bird' in SQL:\n{}",
                line_idx + 1,
                line.trim()
            );
        }
    }

    // Verify runtime SQLite schema defaults via PRAGMA
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("guardrail.sqlite");
    let db = avicull::db::ManifestDb::open(&db_path).expect("open database");
    let raw = std::sync::Arc::clone(db.raw_conn());
    let conn = raw.lock().expect("lock conn");

    for table in ["bundles", "bundle_decisions"] {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", table))
            .expect("prepare pragma");
        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let dflt_val: Option<String> = row.get(4)?;
                Ok((name, dflt_val))
            })
            .expect("query map");

        let mut found_cat = false;
        for r in rows {
            let (name, dflt) = r.expect("row");
            if name == "category" {
                found_cat = true;
                let val = dflt.unwrap_or_default();
                assert!(
                    !val.contains("Bird"),
                    "Table '{}' category column default must not be 'Bird', found: '{}'",
                    table,
                    val
                );
                assert!(
                    val.contains("No_Subject_Detected"),
                    "Table '{}' category column default must be 'No_Subject_Detected', found: '{}'",
                    table,
                    val
                );
            }
        }
        assert!(found_cat, "Table '{}' must have a category column", table);
    }
}

#[test]
fn test_guardrail_catalog_template_no_bird_fallback() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let template_src = std::fs::read_to_string(manifest_dir.join("src/catalog/template.rs"))
        .expect("src/catalog/template.rs must exist");

    // Guardrail: Client-side JS templates must never fall back to 'Bird'
    for (line_idx, line) in template_src.lines().enumerate() {
        if line.contains("|| 'Bird'") || line.contains("|| \"Bird\"") {
            panic!(
                "src/catalog/template.rs line {} violates architectural invariant by falling back to 'Bird':\n{}",
                line_idx + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn test_guardrail_non_avian_entities_never_recognized_as_birds() {
    use avicull::types::is_known_bird_slug;

    let non_avian_test_corpus = [
        "car",
        "automobile",
        "sedan",
        "truck",
        "bicycle",
        "camera",
        "tripod",
        "lens",
        "banyan_tree",
        "ficus_benghalensis",
        "sacred_lotus",
        "nelumbo_nucifera",
        "rhododendron",
        "wild_mushroom",
        "desert_dunes",
        "arid_mountain_valley",
        "waterfall",
        "ocean_coastline",
        "sunset_sky",
        "aurora_borealis",
        "historic_fort_and_palace",
        "historic_stone_temple",
        "close-up_human_portrait",
        "cultural_street_portrait",
        "himalayan_marmot",
        "woolly_hare",
        "wild_yak",
        "canada_lynx",
        "cougar",
        "red_fox",
        "black-tailed_prairie_dog",
        "gaur",
        "unknown",
        "unknown_object",
        "random_noise",
    ];

    for entity in non_avian_test_corpus {
        assert!(
            !is_known_bird_slug(entity),
            "Non-avian entity '{}' must NEVER be recognized by is_known_bird_slug",
            entity
        );
        assert_ne!(
            AssetCategory::from_slug(entity),
            AssetCategory::Bird,
            "Non-avian entity '{}' must NEVER resolve to AssetCategory::Bird",
            entity
        );
    }
}

#[test]
fn test_guardrail_infer_prediction_from_stem_rejects_sentinels_and_botanical() {
    use avicull::organizer::infer_prediction_from_stem;

    let invalid_stems = [
        "IMG_1001__No_Bird_Detected_20260501",
        "IMG_1002__No_Subject_Detected_20260501",
        "IMG_1003_NoBirdDetected_20260501",
        "IMG_1004_NoSubjectDetected_20260501",
        "IMG_3079_Banyan_Tree_20260801_Leh",
        "IMG_3080_Sacred_Lotus_20260801",
    ];

    for stem in invalid_stems {
        let pred = infer_prediction_from_stem(stem);
        assert!(
            pred.is_none(),
            "Stem '{}' must NOT produce a synthesized prediction, got: {:?}",
            stem,
            pred
        );
    }
}

#[test]
fn test_routing_tier_associated_constants_and_dir_name() {
    assert_eq!(RoutingTier::DIR_NEEDS_REVIEW, "_Needs_Review");
    assert_eq!(RoutingTier::PREFIX_NEEDS_REVIEW, "_Needs_Review/");
    assert_eq!(RoutingTier::DIR_NO_SUBJECT_DETECTED, "_No_Subject_Detected");
    assert_eq!(RoutingTier::DIR_NO_BIRD_DETECTED, "_No_Bird_Detected");
    assert_eq!(RoutingTier::DIR_CORRUPTED, "_Corrupted_Images");
    assert_eq!(RoutingTier::DEFAULT_OUTPUT_DIR, "birds_hierarchy");
    assert_eq!(RoutingTier::SENTINEL_NO_BIRD_DETECTED, "_No_Bird_Detected");
    assert_eq!(
        RoutingTier::SENTINEL_NO_SUBJECT_DETECTED,
        "_No_Subject_Detected"
    );

    assert_eq!(RoutingTier::ConfirmedSpecies.dir_name(), None);
    assert_eq!(
        RoutingTier::NeedsReview.dir_name(),
        Some(RoutingTier::DIR_NEEDS_REVIEW)
    );
    assert_eq!(
        RoutingTier::NoBirdDetected.dir_name(),
        Some(RoutingTier::DIR_NO_SUBJECT_DETECTED)
    );
}

#[test]
fn test_guardrail_zero_hardcoded_prominent_fallback_birds_slice() {
    let types_src = std::fs::read_to_string("src/types.rs")
        .or_else(|_| std::fs::read_to_string("../src/types.rs"))
        .expect("Must read src/types.rs");

    assert!(
        !types_src.contains("PROMINENT_FALLBACK_BIRDS"),
        "src/types.rs must not contain ad-hoc PROMINENT_FALLBACK_BIRDS; all birds must be handled by authoritative taxonomy"
    );
}

#[test]
fn test_ebird_taxonomy_comprehensive_global_coverage() {
    use avicull::types::is_known_bird_slug;

    let global_ebird_species = [
        "painted_stork",
        "downy_woodpecker",
        "bald_eagle",
        "jamaican_lizard_cuckoo",
        "tawny_frogmouth",
        "mountain_quail",
        "desert_lark",
        "bearded_vulture",
        "chukar",
        "desert_wheatear",
        "oriental_turtle_dove",
        "eurasian_kestrel",
        "peregrine_falcon",
        "mallard",
        "ruddy_shelduck",
    ];

    for slug in global_ebird_species {
        assert!(
            is_known_bird_slug(slug),
            "Authoritative taxonomy must recognize global eBird species '{}'",
            slug
        );
        assert_eq!(
            AssetCategory::from_slug(slug),
            AssetCategory::Bird,
            "Global eBird species '{}' must resolve to AssetCategory::Bird",
            slug
        );
    }
}

#[test]
fn test_guardrail_catalog_template_defines_frozen_enums() {
    let template_src = std::fs::read_to_string("src/catalog/template.rs")
        .or_else(|_| std::fs::read_to_string("../src/catalog/template.rs"))
        .expect("Must read src/catalog/template.rs");

    assert!(
        template_src.contains("const AssetCategory = Object.freeze"),
        "Catalog template must define frozen AssetCategory enum"
    );
    assert!(
        template_src.contains("const RoutingTier = Object.freeze"),
        "Catalog template must define frozen RoutingTier enum"
    );
    assert!(
        template_src.contains("AssetCategory.NO_SUBJECT_DETECTED"),
        "Catalog template must reference AssetCategory.NO_SUBJECT_DETECTED enum instead of magic string"
    );
}
