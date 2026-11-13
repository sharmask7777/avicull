// tests/test_location_resolver.rs

use avicull::location::gazetteer::GazetteerEngine;
use avicull::location::resolver::{resolve_trip_locations, ResolvedLocation};
use avicull::manifest::ManifestDb;
use std::io::Cursor;
use tempfile::TempDir;

#[test]
fn test_resolve_from_override_distant_coordinates_cutoff() {
    let engine = GazetteerEngine::load_embedded().unwrap();

    // Coordinates in the middle of the Pacific ocean (0.0, -160.0)
    // Should NOT latch onto a continental entry thousands of km away
    let loc = avicull::location::resolver::resolve_from_override("0.0, -160.0", &engine).unwrap();
    assert_eq!(loc.region_code, "");
    assert_eq!(loc.name, "0.0000, -160.0000");

    // Coordinates within 15 km of Leh (34.152, 77.577) should resolve region
    let loc_leh =
        avicull::location::resolver::resolve_from_override("34.152, 77.577", &engine).unwrap();
    assert_eq!(loc_leh.region_code, "IN-LA");
}

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

    // Output should contain candidate prompt text and the entire resolved directory path
    let prompt_output = String::from_utf8(output).unwrap();
    assert!(prompt_output.contains(
        "Resolve location for directory: \"pictures_bangalore-2025-/saul kere apr 19 2026\""
    ));
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
fn test_resolver_explicit_skip_persistence_and_reclassify_hint() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![(
        "pictures_bangalore-2025-/mystery trip apr 2026".to_string(),
        vec!["mystery trip".to_string()],
    )];

    // 1. First run: user skips interactive prompt by entering "s"
    let mut input = Cursor::new("s\n");
    let mut output = Vec::new();

    let resolved = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        true,
        None,
    )
    .unwrap();

    assert!(resolved.is_empty());
    assert!(manifest
        .is_directory_location_skipped("pictures_bangalore-2025-/mystery trip apr 2026")
        .unwrap());

    // 2. Second run: prompt should NOT be re-shown; it should explicitly say it was skipped earlier
    // and provide the exact command to reclassify.
    let mut second_input = Cursor::new(""); // empty input would fail or block if prompt appeared
    let mut second_output = Vec::new();

    let resolved_second = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut second_input,
        &mut second_output,
        true,
        None,
    )
    .unwrap();

    assert!(resolved_second.is_empty());
    let prompt_second = String::from_utf8(second_output).unwrap();
    assert!(
        prompt_second.contains("Skipping location resolution for \"pictures_bangalore-2025-/mystery trip apr 2026\" (previously skipped)."),
        "Output did not indicate previous skip: {}",
        prompt_second
    );
    assert!(
        prompt_second.contains("To assign a location and reclassify this directory, run:"),
        "Output did not contain reclassify intro: {}",
        prompt_second
    );
    assert!(
        prompt_second.contains("avicull location --dir \"pictures_bangalore-2025-/mystery trip apr 2026\" --location \"<LOCATION>\" --reclassify"),
        "Output did not contain expected command: {}",
        prompt_second
    );

    // 3. Setting a location via location command clears skipped state
    let new_loc = avicull::location::resolver::ResolvedLocation {
        name: "Cubbon Park".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 12.9763,
        longitude: 77.5929,
        source: "override_gazetteer".to_string(),
    };
    manifest
        .set_directory_location("pictures_bangalore-2025-/mystery trip apr 2026", &new_loc)
        .unwrap();
    assert!(!manifest
        .is_directory_location_skipped("pictures_bangalore-2025-/mystery trip apr 2026")
        .unwrap());
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
    // Nearest gazetteer entry to (12.919, 77.677) is Saul Kere (within 1km), so region should be inferred as IN-KA
    assert_eq!(loc_coords.region_code, "IN-KA");

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

#[test]
fn test_resolver_diverse_trips_non_interactive() {
    use avicull::location::path_parser::extract_candidate_location_tokens;
    use std::path::Path;

    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let test_cases = vec![
        (
            "pictures_bangalore-2025-/Gir 2026 Feb/selects/1X7A1234.CR2",
            "IN-GJ",
        ),
        (
            "pictures_bangalore-2025-/K Gudi 2025/day 2/1X7A5192.jpg",
            "IN-KA",
        ),
        (
            "pictures_bangalore-2025-/Nagarhole - Mar 2026/1X7A1234.CR2",
            "IN-KA",
        ),
        (
            "pictures_bangalore-2025-/New zealand 2025/rotorua waterfront day 1/1X7A3897.jpg",
            "NZ",
        ),
        (
            "pictures_bangalore-2025-/Saul Kere jan 2025/processed (select)/1X7A0791.jpg",
            "IN-KA",
        ),
        (
            "pictures_bangalore-2025-/coonoor may 2026/1X7A1234.CR2",
            "IN-TN",
        ),
        (
            "pictures_bangalore-2025-/coorg Dec 2025/1X7A1234.CR2",
            "IN-KA",
        ),
        (
            "pictures_bangalore-2025-/jhalana jan 2026/1X7A1234.CR2",
            "IN-RJ",
        ),
        (
            "pictures_bangalore-2025-/kanaka sarovara May 3 2026/1X7A1234.CR2",
            "IN-KA",
        ),
        ("pictures_bangalore-2025-/leh/1X7A1234.CR2", "IN-LA"),
        (
            "pictures_bangalore-2025-/ranthambore oct 2025/processed oct 3 safari/1X7A2731.jpg",
            "IN-RJ",
        ),
    ];

    let base_input = Path::new("pictures_bangalore-2025-");
    let mut trips = Vec::new();
    for (rel_path, _expected_region) in &test_cases {
        let p = Path::new(rel_path);
        let tokens = extract_candidate_location_tokens(p);
        let trip_key = p
            .strip_prefix(base_input)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| p.to_string_lossy().to_string());
        trips.push((trip_key, tokens));
    }

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

    for (i, (rel_path, expected_region)) in test_cases.iter().enumerate() {
        let trip_key = &trips[i].0;
        let loc = resolved.get(trip_key).unwrap_or_else(|| {
            panic!(
                "Trip key '{}' (path '{}') was not resolved by non-interactive resolver. Tokens: {:?}",
                trip_key, rel_path, trips[i].1
            )
        });
        assert_eq!(
            &loc.region_code, expected_region,
            "Path '{}' resolved to '{}', expected '{}'",
            rel_path, loc.region_code, expected_region
        );
    }
}

#[test]
fn test_nested_subdirectories_location_prompt_contains_full_resolved_path() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    // Nested directory structure: "2025/Ladakh/scenery"
    let nested_trip_path = "2025/Ladakh/scenery".to_string();
    let trips = vec![(
        nested_trip_path.clone(),
        vec!["scenery".to_string(), "ladakh".to_string()],
    )];

    let mut input = Cursor::new("s\n"); // skip prompt
    let mut output = Vec::new();

    let _ = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        true, // is_interactive
        None,
    )
    .unwrap();

    let prompt_output = String::from_utf8(output).unwrap();
    // Invariant: The prompt must display the entire resolved path "2025/Ladakh/scenery",
    // not just the leaf directory "scenery".
    assert!(
        prompt_output.contains("Resolve location for directory: \"2025/Ladakh/scenery\""),
        "Expected prompt to contain full resolved path '2025/Ladakh/scenery', but got:\n{}",
        prompt_output
    );
}
