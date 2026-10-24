use avicull::classifier::*;
use avicull::types::*;
use image::{DynamicImage, RgbImage};

#[test]
fn test_posterior_calculation_basic_and_normalization() {
    // 3 species
    let visual_probs = vec![0.70, 0.20, 0.10];
    let geo_priors = vec![0.90, 0.50, 0.0001]; // 3rd species is out of range / vagrant

    let posterior = compute_posterior(&visual_probs, &geo_priors, 1.0, 0.01);
    assert_eq!(posterior.len(), 3);

    // Sum must equal 1.0 within float epsilon
    let sum: f32 = posterior.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-5,
        "Posterior must sum to 1.0, got {}",
        sum
    );

    // Vagrant species (geo prior 0.0001) should be clamped to epsilon 0.01
    // Unnormalized weights:
    // s0: 0.70 * 0.90 = 0.63
    // s1: 0.20 * 0.50 = 0.10
    // s2: 0.10 * 0.01 = 0.001
    // Sum = 0.731
    // s0 = 0.63 / 0.731 = ~0.8618
    // s2 = 0.001 / 0.731 = ~0.00137
    assert!(posterior[0] > 0.85);
    assert!(posterior[2] < 0.01);
}

#[test]
fn test_posterior_downranks_out_of_range_lookalike() {
    // Suppose species 0 is an African bird (visual prob 0.60, but geo prior 0.0)
    // Suppose species 1 is an Indian resident (visual prob 0.40, geo prior 0.95)
    let visual_probs = vec![0.60, 0.40];
    let geo_priors = vec![0.00, 0.95];

    let posterior = compute_posterior(&visual_probs, &geo_priors, 1.0, 0.01);

    // With geo_weight = 1.0:
    // s0 weight = 0.60 * 0.01 = 0.006
    // s1 weight = 0.40 * 0.95 = 0.380
    // s1 should strongly beat s0 in posterior!
    assert!(
        posterior[1] > posterior[0],
        "Resident species must surpass out-of-range lookalike after geo prior"
    );
    assert!(posterior[1] > 0.95);
}

#[test]
fn test_posterior_zero_geo_weight_is_pure_visual() {
    let visual_probs = vec![0.60, 0.40];
    let geo_priors = vec![0.00, 0.95];

    // geo_weight = 0.0 disables geo prior
    let posterior = compute_posterior(&visual_probs, &geo_priors, 0.0, 0.01);
    assert!((posterior[0] - 0.60).abs() < 1e-4);
    assert!((posterior[1] - 0.40).abs() < 1e-4);
}

#[test]
fn test_mock_species_classifier() {
    let classifier = MockSpeciesClassifier::new(vec![SpeciesPrediction {
        taxon_id: "paisto1".to_string(),
        common_name: "Painted Stork".to_string(),
        folder_slug: "Painted_Stork".to_string(),
        scientific_name: "Mycteria leucocephala".to_string(),
        visual_prob: 0.95,
        geo_prior: 0.90,
        combined_score: 0.96,
    }]);

    let img = DynamicImage::ImageRgb8(RgbImage::new(224, 224));
    let (embedding, preds) = classifier
        .classify(&img, None, 1.0)
        .expect("classify failed");
    assert_eq!(preds.len(), 1);
    assert_eq!(preds[0].common_name, "Painted Stork");
    assert_eq!(embedding.len(), 512);
}
