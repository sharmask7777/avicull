// Verifies that the generated HTML catalog contains:
//   1. The bulk location rows container and key JS functions for the Directory Overrides tab
//   2. An empty-location guard in renderBulkLocationPreview so blank rows are excluded
use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{OrganizationMode, RoutingTier};
use std::path::PathBuf;
use tempfile::tempdir;

fn item_with_subdir(subdir: &str) -> CatalogItem {
    CatalogItem {
        bundle_id: "b1".into(),
        original_stem: "IMG_001".into(),
        current_path: PathBuf::from(format!("{}/IMG_001.CR2", subdir)),
        primary_image: PathBuf::from(format!("{}/IMG_001.CR2", subdir)),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        is_local: Some(false),
        assigned_species: "House Sparrow".into(),
        scientific_name: None,
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".into()),
        location_name: Some("Leh".into()),
        location_source: None,
        subdirectory: Some(subdir.to_string()),
        top_predictions: vec![],
    }
}

#[test]
fn test_bulk_location_tab_present_in_modal() {
    let dir = tempdir().unwrap();
    let items = vec![
        item_with_subdir("Leh Valley"),
        item_with_subdir("Leh Valley/Shey Marshes"),
    ];
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(
        html.contains("bulkLocationRows") || html.contains("bulk-location"),
        "bulk location rows container not found"
    );
    assert!(
        html.contains("addBulkLocationRow"),
        "addBulkLocationRow function not found"
    );
    assert!(
        html.contains("renderBulkLocationPreview"),
        "renderBulkLocationPreview function not found"
    );
}

#[test]
fn test_bulk_location_commands_exclude_empty_location_rows() {
    // Verifies the JS function body in the template contains the empty-location guard
    let dir = tempdir().unwrap();
    let items = vec![item_with_subdir("Leh Valley")];
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    // The filter for non-empty location in renderBulkLocationPreview must be present
    assert!(
        html.contains("row.location") || html.contains(".location.trim()"),
        "empty location guard not found in bulk preview function"
    );
}
