use avicull::catalog::{
    generate_html_catalog_with_policy, CatalogItem, CatalogPayload, ThumbnailEmbeddingPolicy,
    CATALOG_FILES_DIR,
};
use avicull::types::{AssetCategory, OrganizationMode, RoutingTier};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_test_item(id: &str, stem: &str, subdir: Option<&str>) -> CatalogItem {
    let current_path = match subdir {
        Some(sub) => PathBuf::from(format!("{sub}/{stem}.CR2")),
        None => PathBuf::from(format!("{stem}.CR2")),
    };
    CatalogItem {
        bundle_id: id.to_string(),
        original_stem: stem.to_string(),
        current_path: current_path.clone(),
        primary_image: current_path,
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        is_local: Some(false),
        assigned_species: "Common Kingfisher".to_string(),
        scientific_name: Some("Alcedo atthis".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.96,
        capture_date: Some("2024-05-15".to_string()),
        location_name: Some("Kaziranga".to_string()),
        location_source: Some("trip_gazetteer".to_string()),
        subdirectory: subdir.map(String::from),
        top_predictions: vec![],
        category: AssetCategory::Bird,
    }
}

fn extract_catalog_payload(html: &str) -> CatalogPayload {
    let json_str = extract_raw_catalog_json(html);
    serde_json::from_str(json_str).expect("valid CatalogPayload json")
}

fn extract_raw_catalog_json(html: &str) -> &str {
    let json_start = html
        .find(r#"<script id="catalog-data" type="application/json">"#)
        .expect("catalog-data start tag");
    let after_start = &html[json_start..];
    let data_start = after_start.find('>').expect("close script tag") + 1;
    let data_end = after_start.find("</script>").expect("script end tag");
    after_start[data_start..data_end].trim()
}

#[test]
fn test_thumbnail_embedding_policy_suppressed_produces_none() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");
    let items = vec![
        make_test_item("b1", "IMG_0001", None),
        make_test_item("b2", "IMG_0002", None),
    ];

    generate_html_catalog_with_policy(
        "Test Trip",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate_html_catalog_with_policy suppressed");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);

    assert_eq!(payload.items.len(), 2);
    for item in &payload.items {
        assert!(
            item.embedded_thumb_data_uri.is_none(),
            "Suppressed policy must produce embedded_thumb_data_uri == None, found {:?}",
            item.embedded_thumb_data_uri
        );
    }
}

#[test]
fn test_parent_rollup_item_resolves_thumb_src_and_preview_url() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");
    let items = vec![make_test_item("b1", "IMG_9999", Some("2024_05_Kaziranga"))];

    generate_html_catalog_with_policy(
        "Parent Rollup",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate_html_catalog_with_policy parent rollup");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);

    assert_eq!(payload.items.len(), 1);
    let item = &payload.items[0];

    let expected_rel_path = format!("2024_05_Kaziranga/{CATALOG_FILES_DIR}/previews/IMG_9999.jpg");
    assert_eq!(
        item.thumb_src, expected_rel_path,
        "Parent rollup item thumb_src must resolve to child preview path"
    );
    assert_eq!(
        item.preview_url, expected_rel_path,
        "Parent rollup item preview_url must resolve to child preview path"
    );
    assert!(item.embedded_thumb_data_uri.is_none());
}

#[test]
fn test_generated_html_contains_lazy_loading_and_async_decoding() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");
    let items = vec![make_test_item("b1", "IMG_0001", None)];

    generate_html_catalog_with_policy(
        "Lazy Load Test",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate catalog");

    let html = fs::read_to_string(&out).expect("read catalog");

    assert!(
        html.contains(r#"loading="lazy""#),
        "Catalog HTML must contain loading=\"lazy\" on image tags"
    );
    assert!(
        html.contains(r#"decoding="async""#),
        "Catalog HTML must contain decoding=\"async\" on image tags"
    );
    assert!(
        html.contains("IntersectionObserver"),
        "Catalog HTML client-side script must feature IntersectionObserver support"
    );
}

#[test]
fn test_size_comparison_suppressed_vs_embedded_1000_items() {
    let dir = tempdir().expect("tempdir");
    let out_suppressed = dir.path().join("catalog_suppressed.html");
    let out_forced = dir.path().join("catalog_forced.html");

    // Build 1,000 items
    let mut items = Vec::with_capacity(1000);
    for i in 0..1000 {
        let stem = format!("{i:04}");
        let id = format!("b{i:04}");
        items.push(CatalogItem {
            bundle_id: id,
            original_stem: stem.clone(),
            current_path: PathBuf::from(format!("{stem}.jpg")),
            primary_image: PathBuf::from(format!("{stem}.jpg")),
            companion_images: vec![],
            sidecar_files: vec![],
            relative_preview_path: None,
            is_local: Some(false),
            assigned_species: "Bird".to_string(),
            scientific_name: None,
            routing_tier: RoutingTier::ConfirmedSpecies,
            confidence: 0.95,
            capture_date: None,
            location_name: None,
            location_source: None,
            subdirectory: None,
            top_predictions: vec![],
            category: AssetCategory::Bird,
        });
    }

    // 1. Generate with Suppressed policy
    generate_html_catalog_with_policy(
        "1000 Photos Suppressed",
        &items,
        OrganizationMode::InPlace,
        &out_suppressed,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate suppressed catalog");

    let html_suppressed = fs::read_to_string(&out_suppressed).expect("read suppressed");
    let payload_suppressed = extract_catalog_payload(&html_suppressed);
    let items_json_suppressed =
        serde_json::to_string(&payload_suppressed.items).expect("serialize items");
    let suppressed_size_bytes = items_json_suppressed.len();

    // 2. Generate with Forced policy and simulated base64 micro-thumbnails (~25 KB each)
    let dummy_base64_thumb = format!("data:image/jpeg;base64,{}", "A".repeat(25_000));
    let mut items_embedded = items.clone();
    for item in &mut items_embedded {
        item.relative_preview_path = Some(dummy_base64_thumb.clone());
    }

    generate_html_catalog_with_policy(
        "1000 Photos Forced",
        &items_embedded,
        OrganizationMode::InPlace,
        &out_forced,
        ThumbnailEmbeddingPolicy::Forced,
    )
    .expect("generate forced catalog");

    let html_forced = fs::read_to_string(&out_forced).expect("read forced");
    let payload_forced = extract_catalog_payload(&html_forced);
    let items_json_forced = serde_json::to_string(&payload_forced.items).expect("serialize items");
    let forced_size_bytes = items_json_forced.len();

    // Spec Invariant:
    // Suppressed JSON payload for 1,000 items is ~360 KB (< 375 KB) vs > 20 MB with base64 thumbnails
    assert!(
        suppressed_size_bytes < 375 * 1024,
        "Suppressed JSON payload for 1,000 items must be < 375 KB, got {} bytes ({:.2} KB)",
        suppressed_size_bytes,
        suppressed_size_bytes as f64 / 1024.0
    );
    assert!(
        forced_size_bytes > 20 * 1024 * 1024,
        "Forced embedded JSON payload for 1,000 items must be > 20 MB, got {} bytes ({:.2} MB)",
        forced_size_bytes,
        forced_size_bytes as f64 / (1024.0 * 1024.0)
    );
}

#[test]
fn test_zero_raw_reads_under_suppressed_policy() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");

    // Point active primary to a non-existent RAW path so any fs::read would fail if executed
    let mut item = make_test_item("b1", "IMG_NONEXISTENT", Some("child_trip"));
    item.current_path = dir.path().join("child_trip/IMG_NONEXISTENT.CR2");
    item.primary_image = item.current_path.clone();

    // Suppressed policy should never attempt to read active_primary
    generate_html_catalog_with_policy(
        "Zero Raw Read Trip",
        &[item],
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("catalog generation must succeed with zero RAW reads");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);
    assert_eq!(payload.items.len(), 1);
    assert!(payload.items[0].embedded_thumb_data_uri.is_none());
}

#[test]
fn test_single_trip_nested_subdirectory_resolves_legacy_root_previews() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");

    // Single-trip catalog: preview exists at root's .avicull/previews
    let legacy_previews = dir
        .path()
        .join(avicull::catalog::LEGACY_CATALOG_FILES_DIR)
        .join("previews");
    fs::create_dir_all(&legacy_previews).expect("create legacy previews dir");
    fs::write(legacy_previews.join("2470-1.jpg"), b"fake jpeg bytes").expect("write fake preview");

    // The item is inside a nested subdirectory of the trip, e.g. "2470 trial"
    let items = vec![make_test_item("b1", "2470-1", Some("2470 trial"))];

    generate_html_catalog_with_policy(
        "Seattle Trip",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate catalog");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);

    assert_eq!(payload.items.len(), 1);
    let item = &payload.items[0];

    let expected = format!(
        "{}/previews/2470-1.jpg",
        avicull::catalog::LEGACY_CATALOG_FILES_DIR
    );
    assert_eq!(
        item.thumb_src, expected,
        "Single trip nested item must resolve to existing root legacy preview path"
    );
    assert_eq!(
        item.preview_url, expected,
        "Single trip nested item preview_url must resolve to existing root legacy preview path"
    );
}

#[test]
fn test_single_trip_nested_subdirectory_resolves_unhidden_root_previews() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");

    // Single-trip catalog: preview exists at root's avicull_catalog_files/previews
    let unhidden_previews = dir
        .path()
        .join(avicull::catalog::CATALOG_FILES_DIR)
        .join("previews");
    fs::create_dir_all(&unhidden_previews).expect("create unhidden previews dir");
    fs::write(unhidden_previews.join("2470-1.jpg"), b"fake jpeg bytes")
        .expect("write fake preview");

    let items = vec![make_test_item("b1", "2470-1", Some("2470 trial"))];

    generate_html_catalog_with_policy(
        "Seattle Trip",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate catalog");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);

    assert_eq!(payload.items.len(), 1);
    let item = &payload.items[0];

    let expected = format!(
        "{}/previews/2470-1.jpg",
        avicull::catalog::CATALOG_FILES_DIR
    );
    assert_eq!(
        item.thumb_src, expected,
        "Single trip nested item must resolve to unhidden root preview path"
    );
    assert_eq!(
        item.preview_url, expected,
        "Single trip nested item preview_url must resolve to unhidden root preview path"
    );
}

#[test]
fn test_parent_rollup_nested_child_subdirectory_resolves_child_root_previews() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("catalog.html");

    // Parent rollup: child trip is "seattle", preview is at seattle/.avicull/previews/2470-1.jpg
    let child_legacy_previews = dir
        .path()
        .join("seattle")
        .join(avicull::catalog::LEGACY_CATALOG_FILES_DIR)
        .join("previews");
    fs::create_dir_all(&child_legacy_previews).expect("create child legacy previews dir");
    fs::write(child_legacy_previews.join("2470-1.jpg"), b"fake jpeg bytes")
        .expect("write fake preview");

    // The item's subdirectory is "seattle/2470 trial"
    let items = vec![make_test_item("b1", "2470-1", Some("seattle/2470 trial"))];

    generate_html_catalog_with_policy(
        "Parent Rollup",
        &items,
        OrganizationMode::InPlace,
        &out,
        ThumbnailEmbeddingPolicy::Suppressed,
    )
    .expect("generate catalog");

    let html = fs::read_to_string(&out).expect("read catalog");
    let payload = extract_catalog_payload(&html);

    assert_eq!(payload.items.len(), 1);
    let item = &payload.items[0];

    let expected = format!(
        "seattle/{}/previews/2470-1.jpg",
        avicull::catalog::LEGACY_CATALOG_FILES_DIR
    );
    assert_eq!(
        item.thumb_src, expected,
        "Parent rollup nested child item must resolve to child trip's preview path"
    );
    assert_eq!(
        item.preview_url, expected,
        "Parent rollup nested child item preview_url must resolve to child trip's preview path"
    );
}
