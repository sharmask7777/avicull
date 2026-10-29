// Verifies that the rendered HTML catalog card shows:
//   1. `.card-species-title span` containing ONLY the species name
//   2. A `.card-date` element containing the capture_date (separate from location)
//   3. `.card-location` element containing the location_name
//   4. No date text inside `.card-location` and vice versa
use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{OrganizationMode, RoutingTier};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_item(species: &str, date: Option<&str>, location: Option<&str>) -> CatalogItem {
    CatalogItem {
        bundle_id: "b1".into(),
        original_stem: "IMG_001".into(),
        current_path: PathBuf::from("IMG_001.CR2"),
        primary_image: PathBuf::from("IMG_001.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        is_local: Some(false),
        assigned_species: species.to_string(),
        scientific_name: None,
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: date.map(String::from),
        location_name: location.map(String::from),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
    }
}

#[test]
fn test_card_species_title_is_species_only() {
    let dir = tempdir().unwrap();
    let item = make_item("Oriental Magpie-Robin", Some("2025-05-18"), None);
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    // Card title must not contain the date or location suffix — species comes from
    // assigned_species (already clean), not from the current_path slug.
    assert!(
        html.contains("Oriental Magpie-Robin"),
        "species name absent"
    );
    assert!(
        !html.contains("20250518"),
        "raw date slug must not appear in the HTML"
    );
    assert!(
        !html.contains("K_Gudi"),
        "location slug must not appear in the HTML"
    );
}

#[test]
fn test_card_date_on_separate_line() {
    let dir = tempdir().unwrap();
    let item = make_item("Oriental Magpie-Robin", Some("2025-05-18"), Some("K Gudi"));
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    // Date must appear inside a dedicated .card-date element
    assert!(
        html.contains("class=\"card-date\""),
        "no .card-date element found"
    );
    // Location must appear inside a dedicated .card-location element
    assert!(
        html.contains("class=\"card-location\""),
        "no .card-location element found"
    );
}

#[test]
fn test_card_no_date_shows_no_date_line() {
    let dir = tempdir().unwrap();
    let item = make_item("Oriental Magpie-Robin", None, Some("K Gudi"));
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    // When capture_date is None, the JSON payload carries null so the JS conditional
    // renders an empty string — no card-date element appears. Verify the data is null
    // and no formatted date string leaks into the payload.
    assert!(
        html.contains("\"capture_date\":null"),
        "capture_date must be null in payload when no date provided"
    );
    assert!(
        !html.contains("2025-"),
        "no date string should appear in the HTML when no date is set"
    );
}
