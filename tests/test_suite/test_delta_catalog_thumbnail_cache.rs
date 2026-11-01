use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::db::ManifestDb;
use avicull::types::{OrganizationMode, RoutingTier};
use std::path::PathBuf;
use tempfile::tempdir;

fn dummy_item(id: &str, stem: &str) -> CatalogItem {
    CatalogItem {
        bundle_id: id.into(),
        original_stem: stem.into(),
        current_path: PathBuf::from(format!("{}.jpg", stem)),
        primary_image: PathBuf::from(format!("{}.jpg", stem)),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        is_local: Some(true),
        assigned_species: "House Sparrow".into(),
        scientific_name: None,
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".into()),
        location_name: Some("Test".into()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
    }
}

#[test]
fn test_manifest_db_persists_and_retrieves_micro_thumbnails() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    // Verify empty at first
    assert!(db.get_thumbnail_data_uri("bundle_1").unwrap().is_none());

    // Store thumbnails
    let sample_uri = "data:image/jpeg;base64,/9j/4AAQSkZJRg==";
    db.store_thumbnail("bundle_1", sample_uri).unwrap();

    // Retrieve single
    assert_eq!(
        db.get_thumbnail_data_uri("bundle_1").unwrap(),
        Some(sample_uri.to_string())
    );

    // Retrieve all map
    let all = db.get_all_thumbnails().unwrap();
    assert_eq!(all.get("bundle_1"), Some(&sample_uri.to_string()));
}

#[test]
fn test_generate_html_catalog_reuses_cached_thumbnails() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join(".avicull/manifest.sqlite");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    let sample_uri = "data:image/jpeg;base64,CACHED_TEST_THUMBNAIL_DATA";
    db.store_thumbnail("b_cached", sample_uri).unwrap();

    let items = vec![dummy_item("b_cached", "IMG_CACHED")];
    let catalog_path = dir.path().join("bird_catalog.html");

    // Pass db or thumbnail cache
    generate_html_catalog(
        "Test Trip",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .unwrap();

    let content = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(
        content.contains("CACHED_TEST_THUMBNAIL_DATA"),
        "Catalog HTML must contain cached thumbnail URI from SQLite"
    );
}
