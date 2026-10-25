// tests/test_location_resolver.rs

use avicull::location::gazetteer::GazetteerEngine;
use avicull::location::resolver::{resolve_trip_locations, ResolvedLocation};
use avicull::manifest::ManifestDb;
use std::io::Cursor;
use tempfile::TempDir;

#[test]
fn test_resolver_with_interactive_input() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![(
        "pictures_bangalore-2025-/saul kere apr 19 2026".to_string(),
        vec!["saul kere".to_string(), "bangalore".to_string()],
    )];

    // User chooses option 1
    let mut input = Cursor::new("1\n");
    let mut output = Vec::new();

    let resolved = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        true, // is_interactive
        None,
    )
    .unwrap();

    assert_eq!(resolved.len(), 1);
    let loc = resolved
        .get("pictures_bangalore-2025-/saul kere apr 19 2026")
        .unwrap();
    assert!(loc.name.to_lowercase().contains("saul kere"));
    assert_eq!(loc.region_code, "IN-KA");

    // Output should contain candidate prompt text
    let prompt_output = String::from_utf8(output).unwrap();
    assert!(prompt_output.contains("[1]"));

    // Second run: should pull from SQLite cache without asking input
    let mut empty_input = Cursor::new("");
    let mut empty_output = Vec::new();
    let cached = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut empty_input,
        &mut empty_output,
        true,
        None,
    )
    .unwrap();
    assert_eq!(cached.len(), 1);
    let cached_loc = cached
        .get("pictures_bangalore-2025-/saul kere apr 19 2026")
        .unwrap();
    assert_eq!(cached_loc, loc);
    assert!(empty_output.is_empty());
}

#[test]
fn test_resolver_non_interactive_confident_and_skipped() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![
        (
            "trips/high_confidence_trip".to_string(),
            vec!["saul kere".to_string()],
        ),
        (
            "trips/unknown_gibberish_trip".to_string(),
            vec!["xyz123abc456foobar".to_string()],
        ),
    ];

    let mut input = Cursor::new("");
    let mut output = Vec::new();

    let resolved = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        false, // non-interactive
        None,
    )
    .unwrap();

    // High confidence should be resolved (>0.85 score)
    assert!(resolved.contains_key("trips/high_confidence_trip"));
    let loc = resolved.get("trips/high_confidence_trip").unwrap();
    assert!(loc.name.to_lowercase().contains("saul kere"));
    assert_eq!(loc.region_code, "IN-KA");

    // Unknown gibberish should be skipped (not present in resolved map)
    assert!(!resolved.contains_key("trips/unknown_gibberish_trip"));
}

#[test]
fn test_resolver_with_location_override_coordinates_and_gazetteer() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![("trips/some_trip".to_string(), vec![])];

    let mut input = Cursor::new("");
    let mut output = Vec::new();

    // Test coordinate override
    let resolved_coords = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        false,
        Some("12.919, 77.677"),
    )
    .unwrap();

    let loc_coords = resolved_coords.get("trips/some_trip").unwrap();
    assert!((loc_coords.latitude - 12.919).abs() < 1e-4);
    assert!((loc_coords.longitude - 77.677).abs() < 1e-4);

    // Test named gazetteer override
    let resolved_name = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        false,
        Some("Kruger"),
    )
    .unwrap();

    let loc_kruger = resolved_name.get("trips/some_trip").unwrap();
    assert!(loc_kruger.name.to_lowercase().contains("kruger"));
}

#[test]
fn test_manifest_directory_location_cache() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();

    let key = "pictures/leh_2024";
    assert_eq!(manifest.get_directory_location(key).unwrap(), None);

    let sample_loc = ResolvedLocation {
        name: "Leh, Ladakh, India".to_string(),
        region_code: "IN-LA".to_string(),
        latitude: 34.1526,
        longitude: 77.5771,
        source: "test".to_string(),
    };

    manifest.set_directory_location(key, &sample_loc).unwrap();

    let fetched = manifest.get_directory_location(key).unwrap();
    assert_eq!(fetched, Some(sample_loc));
}
