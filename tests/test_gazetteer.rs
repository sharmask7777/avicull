// tests/test_gazetteer.rs

use avicull::location::gazetteer::{GazetteerEngine, GazetteerEntry};

#[test]
fn test_gazetteer_search_exact_and_fuzzy() {
    let engine = GazetteerEngine::load_embedded().expect("should load embedded gazetteer");

    // Search exact hotspot
    let matches = engine.search("saul kere", 4);
    assert!(!matches.is_empty());
    assert!(matches[0].0.name.to_lowercase().contains("saul kere"));
    assert_eq!(matches[0].0.region_code, "IN-KA");

    // Search city name with typo/variant
    let matches_blr = engine.search("bangalore", 4);
    assert!(!matches_blr.is_empty());
    assert!(matches_blr.iter().any(|(e, _)| e.region_code == "IN-KA"));

    // Search international reserve
    let matches_kruger = engine.search("kruger", 4);
    assert!(!matches_kruger.is_empty());
    assert!(matches_kruger[0].0.name.to_lowercase().contains("kruger"));
}

#[test]
fn test_gazetteer_entry_deserialization() {
    let json_lat_lon = r#"{
        "name": "Test Spot",
        "region_code": "IN-KA",
        "lat": 12.34,
        "lon": 56.78,
        "category": "hotspot"
    }"#;
    let entry1: GazetteerEntry = serde_json::from_str(json_lat_lon).unwrap();
    assert_eq!(entry1.name, "Test Spot");
    assert_eq!(entry1.region_code, "IN-KA");
    assert!((entry1.latitude - 12.34).abs() < 1e-6);
    assert!((entry1.longitude - 56.78).abs() < 1e-6);
    assert_eq!(entry1.category, "hotspot");

    let json_latitude_longitude = r#"{
        "name": "Test Spot 2",
        "region_code": "US-FL",
        "latitude": 25.5,
        "longitude": -80.5,
        "category": "reserve"
    }"#;
    let entry2: GazetteerEntry = serde_json::from_str(json_latitude_longitude).unwrap();
    assert_eq!(entry2.name, "Test Spot 2");
    assert_eq!(entry2.region_code, "US-FL");
    assert!((entry2.latitude - 25.5).abs() < 1e-6);
    assert!((entry2.longitude - (-80.5)).abs() < 1e-6);
    assert_eq!(entry2.category, "reserve");
}

#[test]
fn test_gazetteer_search_limits_and_empty() {
    let engine = GazetteerEngine::load_embedded().expect("should load embedded gazetteer");

    // Empty query
    let empty = engine.search("", 4);
    assert!(empty.is_empty());

    let spaces = engine.search("   ", 4);
    assert!(spaces.is_empty());

    // Limit check
    let matches = engine.search("lake", 2);
    assert!(matches.len() <= 2);
}

#[test]
fn test_gazetteer_ranking_exact_over_fuzzy() {
    let entries = vec![
        GazetteerEntry {
            name: "Kruger National Park".to_string(),
            region_code: "ZA-MP".to_string(),
            latitude: -24.0,
            longitude: 31.5,
            category: "reserve".to_string(),
        },
        GazetteerEntry {
            name: "Kruger".to_string(),
            region_code: "ZA-MP".to_string(),
            latitude: -24.0,
            longitude: 31.5,
            category: "hotspot".to_string(),
        },
    ];
    let engine = GazetteerEngine::from_entries(entries);
    let results = engine.search("kruger", 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0.name, "Kruger"); // Exact match should be higher than partial match
}

#[test]
fn test_trigram_similarity_math() {
    use avicull::location::gazetteer::trigram_similarity;

    // Identical strings -> 1.0
    assert!((trigram_similarity("kruger", "kruger") - 1.0).abs() < 1e-6);

    // Completely disjoint strings -> 0.0
    assert!((trigram_similarity("abc", "xyz") - 0.0).abs() < 1e-6);

    // Typo similarity
    let sim = trigram_similarity("ranthambore", "ranthambhore");
    assert!(sim > 0.70 && sim < 0.85);
}

#[test]
fn test_gazetteer_accent_and_diacritic_normalization() {
    let entries = vec![GazetteerEntry {
        name: "São Paulo".to_string(),
        region_code: "BR-SP".to_string(),
        latitude: -23.55,
        longitude: -46.63,
        category: "city".to_string(),
    }];
    let engine = GazetteerEngine::from_entries(entries);

    // Search without accent
    let matches = engine.search("sao paulo", 2);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].0.name, "São Paulo");
    assert!(matches[0].1 >= 0.95);

    // Search with accent
    let matches_accent = engine.search("São Paulo", 2);
    assert!(!matches_accent.is_empty());
    assert_eq!(matches_accent[0].0.name, "São Paulo");
}

#[test]
fn test_gazetteer_category_boost_priority() {
    let entries = vec![
        GazetteerEntry {
            name: "Ranthambore".to_string(),
            region_code: "IN-RJ".to_string(),
            latitude: 26.0,
            longitude: 76.5,
            category: "city".to_string(),
        },
        GazetteerEntry {
            name: "Ranthambore".to_string(),
            region_code: "IN-RJ".to_string(),
            latitude: 26.01,
            longitude: 76.51,
            category: "hotspot".to_string(),
        },
    ];
    let engine = GazetteerEngine::from_entries(entries);
    let results = engine.search("ranthambore", 2);
    assert_eq!(results.len(), 2);
    // Hotspot category should be boosted ahead of city
    assert_eq!(results[0].0.category, "hotspot");
    assert_eq!(results[1].0.category, "city");
}
