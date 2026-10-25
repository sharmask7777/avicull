use avicull::classifier::{
    compute_posterior, get_regional_prior_multiplier, load_embedded_regional_checklists,
    EMBEDDED_REGIONAL_CHECKLISTS,
};
use std::collections::{HashMap, HashSet};

#[test]
fn test_regional_prior_penalizes_out_of_range_lookalikes() {
    let mut checklists = HashMap::new();
    let mut karnataka_birds = HashSet::new();
    karnataka_birds.insert("brakit1".to_string()); // Brahminy Kite
    karnataka_birds.insert("indcor1".to_string()); // Indian Cormorant
    checklists.insert("IN-KA".to_string(), karnataka_birds);

    // Brahminy Kite in Karnataka: valid native species
    let kite_prior = get_regional_prior_multiplier(Some("IN-KA"), "brakit1", &checklists);
    assert_eq!(kite_prior, 1.0);

    // Bald Eagle in Karnataka: not on checklist
    let bald_eagle_prior = get_regional_prior_multiplier(Some("IN-KA"), "baleag", &checklists);
    assert_eq!(bald_eagle_prior, 0.001);

    // Black Swan in Karnataka: not on checklist
    let swan_prior = get_regional_prior_multiplier(Some("IN-KA"), "blkswa", &checklists);
    assert_eq!(swan_prior, 0.001);

    // Unassigned location: neutral prior
    let unassigned_prior = get_regional_prior_multiplier(None, "baleag", &checklists);
    assert_eq!(unassigned_prior, 1.0);
}

#[test]
fn test_embedded_regional_checklists_loading() {
    let checklists = load_embedded_regional_checklists();
    assert!(checklists.contains_key("IN-KA"));
    assert!(checklists.contains_key("IN-LA"));
    assert!(checklists.contains_key("US-FL"));

    let karnataka = checklists.get("IN-KA").unwrap();
    assert!(karnataka.contains("brakit1")); // Brahminy Kite is native to KA
    assert!(!karnataka.contains("baleag")); // Bald Eagle is NOT native to KA

    let kite_prior = get_regional_prior_multiplier(Some("IN-KA"), "brakit1", &checklists);
    assert_eq!(kite_prior, 1.0);

    let eagle_prior = get_regional_prior_multiplier(Some("IN-KA"), "baleag", &checklists);
    assert_eq!(eagle_prior, 0.001);

    // Verify static LazyLock
    let lazy_kite =
        get_regional_prior_multiplier(Some("IN-KA"), "brakit1", &EMBEDDED_REGIONAL_CHECKLISTS);
    assert_eq!(lazy_kite, 1.0);

    let lazy_eagle =
        get_regional_prior_multiplier(Some("IN-KA"), "baleag", &EMBEDDED_REGIONAL_CHECKLISTS);
    assert_eq!(lazy_eagle, 0.001);
}

#[test]
fn test_regional_prior_downranks_in_posterior_computation() {
    let checklists = load_embedded_regional_checklists();

    // Out-of-range bird has higher visual probability than native bird
    // e.g. baleag (Bald Eagle) visual prob 0.60 vs brakit1 (Brahminy Kite) visual prob 0.40
    let visual_probs = vec![0.60f32, 0.40f32];
    let taxon_ids = ["baleag", "brakit1"];

    let region_code = "IN-KA";
    let adjusted_priors: Vec<f32> = taxon_ids
        .iter()
        .map(|id| get_regional_prior_multiplier(Some(region_code), id, &checklists) as f32)
        .collect();

    let posterior = compute_posterior(&visual_probs, &adjusted_priors, 1.0, 0.0001);

    // Brahminy Kite (index 1) should now strongly dominate Bald Eagle (index 0)
    assert!(
        posterior[1] > posterior[0],
        "Native species must beat out-of-range lookalike after regional checklist multiplier"
    );
    assert!(
        posterior[1] > 0.99,
        "Native species posterior should be > 0.99 due to 1000x penalty on out-of-range"
    );
}
