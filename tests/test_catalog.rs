use avicull::catalog::{generate_html_catalog, CatalogItem, ClassifiedBundleItem};
use avicull::types::{OrganizationMode, RoutingTier, SpeciesPrediction};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_generate_html_catalog_basic() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item1 = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "IMG_1001".to_string(),
        current_path: PathBuf::from("IMG_1001_House_Sparrow_20261025_Leh.CR2"),
        primary_image: dir.path().join("IMG_1001_House_Sparrow_20261025_Leh.CR2"),
        companion_images: vec![dir.path().join("IMG_1001_House_Sparrow_20261025_Leh.JPG")],
        sidecar_files: vec![dir
            .path()
            .join("IMG_1001_House_Sparrow_20261025_Leh.CR2.xmp")],
        relative_preview_path: Some("IMG_1001_House_Sparrow_20261025_Leh.JPG".to_string()),
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.94,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "houspa".to_string(),
            common_name: "House Sparrow".to_string(),
            folder_slug: "House_Sparrow".to_string(),
            scientific_name: "Passer domesticus".to_string(),
            visual_prob: 0.95,
            geo_prior: 0.90,
            combined_score: 0.94,
        }],
        is_local: None,
    };

    let item2 = ClassifiedBundleItem {
        bundle_id: "bundle-2".to_string(),
        original_stem: "IMG_1002".to_string(),
        current_path: PathBuf::from("IMG_1002_Needs_Review_Black_Redstart_20261025_Leh.CR2"),
        primary_image: dir
            .path()
            .join("IMG_1002_Needs_Review_Black_Redstart_20261025_Leh.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Black Redstart".to_string(),
        scientific_name: Some("Phoenicurus ochruros".to_string()),
        routing_tier: RoutingTier::NeedsReview,
        confidence: 0.52,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "blared1".to_string(),
            common_name: "Black Redstart".to_string(),
            folder_slug: "Black_Redstart".to_string(),
            scientific_name: "Phoenicurus ochruros".to_string(),
            visual_prob: 0.55,
            geo_prior: 0.48,
            combined_score: 0.52,
        }],
        is_local: None,
    };

    let item3 = CatalogItem {
        bundle_id: "bundle-3".to_string(),
        original_stem: "IMG_1003".to_string(),
        current_path: PathBuf::from("IMG_1003_No_Bird_Detected_20261025_Leh.CR2"),
        primary_image: dir
            .path()
            .join("IMG_1003_No_Bird_Detected_20261025_Leh.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "_No_Bird_Detected".to_string(),
        scientific_name: None,
        routing_tier: RoutingTier::NoBirdDetected,
        confidence: 0.12,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    let items = vec![item1, item2, item3];

    generate_html_catalog(
        "Leh Birding Trip",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog should succeed");

    assert!(
        catalog_path.exists(),
        "bird_catalog.html must exist on disk"
    );

    let html_content = fs::read_to_string(&catalog_path).expect("read bird_catalog.html");

    // HTML5 validity
    assert!(html_content.contains("<!DOCTYPE html>"));
    assert!(html_content.contains("<html"));
    assert!(html_content.contains("</html>"));

    // Check embedded JSON catalog data
    assert!(html_content.contains(r#"<script id="catalog-data" type="application/json">"#));

    let json_start = html_content
        .find(r#"<script id="catalog-data" type="application/json">"#)
        .expect("json script start");
    let after_start = &html_content[json_start..];
    let data_start = after_start.find('>').expect("close tag") + 1;
    let data_end = after_start.find("</script>").expect("script end");
    let embedded_json = &after_start[data_start..data_end];

    let parsed: serde_json::Value =
        serde_json::from_str(embedded_json).expect("embedded JSON must be valid");

    assert_eq!(parsed["trip_name"], "Leh Birding Trip");
    assert_eq!(parsed["mode"], "in-place");
    assert_eq!(parsed["stats"]["total_photos"], 3);
    assert_eq!(parsed["stats"]["confirmed_count"], 1);
    assert_eq!(parsed["stats"]["needs_review_count"], 1);
    assert_eq!(parsed["stats"]["no_bird_count"], 1);
    assert_eq!(parsed["stats"]["species_count"], 2);

    // Check interactive UI features: search, filter, mislabel correction, onerror fallback
    assert!(
        html_content.contains("handleImageError"),
        "Must include image error handler for cloud eviction"
    );
    assert!(
        html_content.contains("apply_corrections.sh"),
        "Must include downloadable bash script generation"
    );
    assert!(
        html_content.contains("mv "),
        "Must include shell mv command generation logic"
    );
}

#[test]
fn test_generate_html_catalog_raw_preview_extraction() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    // Create a mock RAW file containing valid SOI and EOI JPEG markers
    let raw_file = dir.path().join("IMG_2001.CR2");
    let mut raw_bytes = vec![0x00; 100]; // padding header
    raw_bytes.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
    raw_bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    raw_bytes.extend_from_slice(&[0xFF, 0xD9]); // EOI
    raw_bytes.extend_from_slice(&[0x00; 50]); // trailing padding

    fs::write(&raw_file, &raw_bytes).expect("write mock RAW file");

    let item = CatalogItem {
        bundle_id: "bundle-raw".to_string(),
        original_stem: "IMG_2001".to_string(),
        current_path: PathBuf::from("IMG_2001_Tibetan_Snowcock_20261025_Leh.CR2"),
        primary_image: raw_file,
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None, // Missing preview path should trigger extraction
        assigned_species: "Tibetan Snowcock".to_string(),
        scientific_name: Some("Tetraogallus tibetanus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.98,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "RAW Extraction Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog with RAW preview");

    let extracted_preview = dir
        .path()
        .join(".avicull")
        .join("previews")
        .join("IMG_2001.jpg");
    assert!(
        extracted_preview.exists(),
        "Embedded preview JPEG must be extracted into .avicull/previews/IMG_2001.jpg"
    );

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");
    assert!(
        html_content.contains(".avicull/previews/IMG_2001.jpg"),
        "Catalog HTML must reference the extracted thumbnail preview"
    );
}

#[test]
fn test_generate_html_catalog_cloud_evicted_error_handling() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    // Primary image does not exist on disk (cloud-evicted file)
    let non_existent_image = dir.path().join("IMG_EVICTED.CR2");

    let item = CatalogItem {
        bundle_id: "bundle-evicted".to_string(),
        original_stem: "IMG_EVICTED".to_string(),
        current_path: PathBuf::from("IMG_EVICTED_Lammergeier_20261025_Leh.CR2"),
        primary_image: non_existent_image,
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Lammergeier".to_string(),
        scientific_name: Some("Gypaetus barbatus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.91,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    // Should succeed gracefully despite unreadable cloud-evicted image bytes
    generate_html_catalog(
        "Cloud Evicted Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog should succeed on cloud-evicted items");

    assert!(catalog_path.exists());
    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");
    assert!(html_content.contains("IMG_EVICTED"));
    assert!(html_content.contains("handleImageError"));
}

#[test]
fn test_generate_html_catalog_hierarchy_mode() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-h".to_string(),
        original_stem: "IMG_3001".to_string(),
        current_path: PathBuf::from("House_Sparrow/Day1__IMG_3001_20261025_Leh_0.95.CR2"),
        primary_image: dir
            .path()
            .join("House_Sparrow/Day1__IMG_3001_20261025_Leh_0.95.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "Hierarchy Trip",
        &[item],
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog hierarchy mode");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");
    assert!(html_content.contains(r#""mode":"hierarchy""#));
}

#[test]
fn test_resolve_or_extract_preview_inplace_renamed() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    // Case 1: In-place renamed RAW file (primary_image does not exist, current_path exists)
    let old_raw = dir.path().join("IMG_2002.CR2");
    let renamed_raw = dir
        .path()
        .join("IMG_2002_Tibetan_Snowcock_20261025_Leh.CR2");

    let mut raw_bytes = vec![0x00; 100];
    raw_bytes.extend_from_slice(&[0xFF, 0xD8]); // SOI
    raw_bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // APP0 marker
    raw_bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    raw_bytes.extend_from_slice(&[0xFF, 0xD9]); // EOI
    raw_bytes.extend_from_slice(&[0x00; 50]);
    fs::write(&renamed_raw, &raw_bytes).expect("write renamed RAW");

    let raw_item = CatalogItem {
        bundle_id: "bundle-raw-inplace".to_string(),
        original_stem: "IMG_2002".to_string(),
        current_path: renamed_raw,
        primary_image: old_raw, // Non-existent original path
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Tibetan Snowcock".to_string(),
        scientific_name: Some("Tetraogallus tibetanus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.98,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    // Case 2: In-place renamed web image (JPEG)
    let old_jpg = dir.path().join("IMG_2003.JPG");
    let renamed_jpg = dir.path().join("IMG_2003_House_Sparrow.JPG");
    fs::write(&renamed_jpg, b"dummy jpeg bytes").expect("write renamed JPG");

    let jpg_item = CatalogItem {
        bundle_id: "bundle-jpg-inplace".to_string(),
        original_stem: "IMG_2003".to_string(),
        current_path: renamed_jpg,
        primary_image: old_jpg, // Non-existent original path
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "InPlace Preview Trip",
        &[raw_item, jpg_item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // RAW preview should have been extracted using the active on-disk path
    let extracted_preview = dir
        .path()
        .join(".avicull")
        .join("previews")
        .join("IMG_2002.jpg");
    assert!(
        extracted_preview.exists(),
        "Embedded preview must be extracted from active on-disk RAW file"
    );
    assert!(html_content.contains(".avicull/previews/IMG_2002.jpg"));

    // Web image preview should resolve to the active on-disk path
    assert!(
        html_content.contains("IMG_2003_House_Sparrow.JPG"),
        "Preview URL must resolve to the active renamed path"
    );
}

#[test]
fn test_mislabel_correction_script_generation_sidecars_and_extensions() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "IMG_1001".to_string(),
        current_path: PathBuf::from("birds_hierarchy/House_Sparrow/IMG_1001.CR2"),
        primary_image: dir.path().join("IMG_1001.CR2"),
        companion_images: vec![dir.path().join("IMG_1001.JPG")],
        sidecar_files: vec![dir.path().join("IMG_1001.CR2.xmp")],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "Script Test Trip",
        &[item],
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // 1. Compound extension must use regex extraction without stem-stripping bug
    assert!(
        html_content.contains(r#"filename.match(/(\.[a-zA-Z0-9]+(?:\.xmp)?)$/i)"#),
        "getCompoundExtension must use regex to capture compound extensions"
    );
    assert!(
        html_content.contains("getCompoundExtension(scFilename)"),
        "getCompoundExtension must take filename directly without stem parameter"
    );

    // 2. Hierarchy mode must move sidecars and companions along with primary
    assert!(
        html_content.contains(r#"commands.push(`mv "${sc}" "${targetDir}/${scFilename}"`);"#),
        "Hierarchy mode must emit mv commands for companion and sidecar files"
    );

    // 3. Catalog directory must be embedded in payload and used in commands & batch script
    assert!(
        html_content.contains("catalog_dir"),
        "Catalog payload must contain catalog_dir"
    );
    assert!(
        html_content.contains(r#"commands.push(`cd "${CATALOG_DATA.catalog_dir}"`);"#),
        "Modal commands must include cd to catalog_dir so mv runs successfully from any directory"
    );
    assert!(
        html_content.contains(r#"cd "${CATALOG_DATA.catalog_dir}""#),
        "Batch script apply_corrections.sh must change directory to catalog_dir"
    );
}
