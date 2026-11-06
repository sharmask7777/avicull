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

    // Search additional wildlife hotspots and global regions
    let matches_gir = engine.search("gir", 4);
    assert!(!matches_gir.is_empty());
    assert_eq!(matches_gir[0].0.region_code, "IN-GJ");

    let matches_kgudi = engine.search("k gudi", 4);
    assert!(!matches_kgudi.is_empty());
    assert_eq!(matches_kgudi[0].0.region_code, "IN-KA");

    let matches_nz = engine.search("new zealand", 4);
    assert!(!matches_nz.is_empty());
    assert_eq!(matches_nz[0].0.region_code, "NZ");

    let matches_coonoor = engine.search("coonoor", 4);
    assert!(!matches_coonoor.is_empty());
    assert_eq!(matches_coonoor[0].0.region_code, "IN-TN");

    let matches_coorg = engine.search("coorg", 4);
    assert!(!matches_coorg.is_empty());
    assert_eq!(matches_coorg[0].0.region_code, "IN-KA");

    let matches_jhalana = engine.search("jhalana", 4);
    assert!(!matches_jhalana.is_empty());
    assert_eq!(matches_jhalana[0].0.region_code, "IN-RJ");
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

#[test]
fn test_gazetteer_find_nearest_haversine() {
    let engine = GazetteerEngine::load_embedded().expect("should load embedded gazetteer");

    // Coordinates right next to Saul Kere lake in Bengaluru (12.912, 77.684)
    let (nearest_saul, dist_saul) = engine
        .find_nearest(12.9123, 77.6845, Some(50.0))
        .expect("should find nearest hotspot to Saul Kere coords");
    assert_eq!(nearest_saul.region_code, "IN-KA");
    assert!(
        dist_saul < 1.0,
        "Distance should be under 1 km, got {dist_saul}"
    );

    // Coordinates in Leh / Ladakh (34.152, 77.577)
    let (nearest_leh, dist_leh) = engine
        .find_nearest(34.1526, 77.5771, Some(100.0))
        .expect("should find nearest hotspot to Leh coords");
    assert_eq!(nearest_leh.region_code, "IN-LA");
    assert!(
        dist_leh < 10.0,
        "Distance to Leh should be under 10 km, got {dist_leh}"
    );

    // Coordinates in the Everglades, Florida (25.2866, -80.8987)
    let (nearest_fl, dist_fl) = engine
        .find_nearest(25.2866, -80.8987, Some(100.0))
        .expect("should find Everglades");
    assert_eq!(nearest_fl.region_code, "US-FL");
    assert!(
        dist_fl < 5.0,
        "Distance to Everglades should be under 5 km, got {dist_fl}"
    );

    // Max distance cutoff test (Middle of Pacific ocean should find nothing within 100km)
    let pacific = engine.find_nearest(0.0, -160.0, Some(100.0));
    assert!(
        pacific.is_none(),
        "Middle of Pacific should exceed 100km max distance"
    );
}
