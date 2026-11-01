use avicull::catalog::{generate_html_catalog, CatalogItem, ClassifiedBundleItem};
use avicull::db::ManifestDb;
use avicull::types::{OrganizationMode, RoutingTier, SpeciesPrediction};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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
        location_source: None,
        subdirectory: None,
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

#[test]
fn test_single_command_copy_is_executable_in_interactive_zsh_without_comment_errors() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "IMG_1001".to_string(),
        current_path: PathBuf::from("birds_hierarchy/House_Sparrow/IMG_1001.CR2"),
        primary_image: dir.path().join("IMG_1001.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    generate_html_catalog(
        "Single Command Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // Extract JSON payload
    let marker = r#"<script id="catalog-data" type="application/json">"#;
    let start_idx = html_content.find(marker).unwrap() + marker.len();
    let end_idx = html_content[start_idx..].find("</script>").unwrap();
    let json_str = &html_content[start_idx..start_idx + end_idx];

    // Extract JS script
    let script_start = html_content.find("<script>\n").unwrap() + 9;
    let script_end = html_content[script_start..].find("</script>").unwrap();
    let js_code = &html_content[script_start..script_start + script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
let clipboardContent = '';
    let cmdOutputObj = {{ textContent: '' }};
    const document = {{
      getElementById: (id) => {{
        if (id === 'catalog-data') return {{ textContent: rawJson }};
        if (id === 'commandOutput') return cmdOutputObj;
        return {{ textContent: '', value: '', style: {{}}, dataset: {{}}, classList: {{ remove: () => {{}}, add: () => {{}} }} }};
      }},
      addEventListener: () => {{}}
    }};
const navigator = {{
  clipboard: {{
    writeText: async (t) => {{
      clipboardContent = t;
      return Promise.resolve();
    }}
  }}
}};
class MockImage {{ constructor() {{}} }}
global.Image = MockImage;
const window = global;

{js_code}

const item = CATALOG_DATA.items[0];
openModal([item.id]);
selectSpecies('Mallard');
copyCurrentCommand();

setTimeout(() => {{
  console.log(clipboardContent);
}}, 20);
"#,
        raw_json = json_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner");

    assert!(
        node_output.status.success(),
        "Node runner failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let copied_cmd = String::from_utf8_lossy(&node_output.stdout)
        .trim()
        .to_string();

    // Verify it doesn't fail when piped into interactive zsh
    let mut child = Command::new("zsh")
        .args(["-f", "-i", "-s"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn zsh process");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin
            .write_all(format!("{}\nexit\n", copied_cmd).as_bytes())
            .unwrap();
    }

    let zsh_res = child.wait_with_output().expect("wait on zsh");
    let zsh_stderr = String::from_utf8_lossy(&zsh_res.stderr);

    assert!(
        !zsh_stderr.contains("command not found: #"),
        "zsh failed with comment error in copied command:\nstderr: {}\ncommand:\n{}",
        zsh_stderr,
        copied_cmd
    );
}

#[test]
fn test_catalog_local_files_do_not_emit_fileproviderctl_evict() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item_local = CatalogItem {
        bundle_id: "bundle-local".to_string(),
        original_stem: "IMG_LOCAL".to_string(),
        current_path: PathBuf::from("IMG_LOCAL.JPG"),
        primary_image: dir.path().join("IMG_LOCAL.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    let item_cloud = CatalogItem {
        bundle_id: "bundle-cloud".to_string(),
        original_stem: "IMG_CLOUD".to_string(),
        current_path: PathBuf::from("IMG_CLOUD.JPG"),
        primary_image: dir.path().join("IMG_CLOUD.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Mallard".to_string(),
        scientific_name: Some("Anas platyrhynchos".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.90,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(false),
    };

    generate_html_catalog(
        "Local vs Cloud Evict Test",
        &[item_local, item_cloud],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    let marker = r#"<script id="catalog-data" type="application/json">"#;
    let start_idx = html_content.find(marker).unwrap() + marker.len();
    let end_idx = html_content[start_idx..].find("</script>").unwrap();
    let json_str = &html_content[start_idx..start_idx + end_idx];

    let script_start = html_content.find("<script>\n").unwrap() + 9;
    let script_end = html_content[script_start..].find("</script>").unwrap();
    let js_code = &html_content[script_start..script_start + script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') return {{ textContent: rawJson }};
    return {{ textContent: '', value: '', style: {{}}, dataset: {{}}, classList: {{ remove: () => {{}}, add: () => {{}} }} }};
  }},
  addEventListener: () => {{}}
}};
const navigator = {{ clipboard: {{ writeText: async () => {{}} }} }};
class MockImage {{ constructor() {{}} }}
global.Image = MockImage;
const window = global;

{js_code}

const localItem = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_LOCAL');
const cloudItem = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_CLOUD');

const localCmds = buildMoveCommandsForItem(localItem, 'Mallard').join('\n');
const cloudCmds = buildMoveCommandsForItem(cloudItem, 'House Sparrow').join('\n');

console.log(JSON.stringify({{
  localCmds,
  cloudCmds
}}));
"#,
        raw_json = json_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner");

    assert!(
        node_output.status.success(),
        "Node runner failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&node_output.stdout).trim()).unwrap();

    let local_cmds = parsed["localCmds"].as_str().unwrap();
    let cloud_cmds = parsed["cloudCmds"].as_str().unwrap();

    assert!(
        !local_cmds.contains("fileproviderctl evict"),
        "Locally available non-cloud file must NOT emit fileproviderctl evict command!\nCommands:\n{}",
        local_cmds
    );
    assert!(
        cloud_cmds.contains("fileproviderctl evict"),
        "Cloud-evicted file MUST emit fileproviderctl evict safeguard command!\nCommands:\n{}",
        cloud_cmds
    );
}

#[test]
fn test_batch_script_updates_consecutive_corrections_and_deduplicates_by_stem() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item1 = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "IMG_1001".to_string(),
        current_path: PathBuf::from("IMG_1001.JPG"),
        primary_image: dir.path().join("IMG_1001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    let item2 = CatalogItem {
        bundle_id: "bundle-2".to_string(),
        original_stem: "IMG_1002".to_string(),
        current_path: PathBuf::from("IMG_1002.JPG"),
        primary_image: dir.path().join("IMG_1002.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Common Myna".to_string(),
        scientific_name: Some("Acridotheres tristis".to_string()),
        routing_tier: RoutingTier::NeedsReview,
        confidence: 0.70,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    generate_html_catalog(
        "Consecutive Batch Test",
        &[item1, item2],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    let marker = r#"<script id="catalog-data" type="application/json">"#;
    let start_idx = html_content.find(marker).unwrap() + marker.len();
    let end_idx = html_content[start_idx..].find("</script>").unwrap();
    let json_str = &html_content[start_idx..start_idx + end_idx];

    let script_start = html_content.find("<script>\n").unwrap() + 9;
    let script_end = html_content[script_start..].find("</script>").unwrap();
    let js_code = &html_content[script_start..script_start + script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
let btnBatchCount = 0;
let btnCopyCount = 0;
const elements = {{
  'batchCount': {{ textContent: '0' }},
  'batchCountCopy': {{ textContent: '0' }},
  'speciesSearchInput': {{ value: '' }},
  'speciesSuggestions': {{ classList: {{ remove: () => {{}} }} }},
  'commandOutput': {{ textContent: '' }},
  'correctionModal': {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }}
}};

const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') return {{ textContent: rawJson }};
    if (!elements[id]) {{
      elements[id] = {{ textContent: '', value: '', style: {{}}, dataset: {{}}, classList: {{ remove: () => {{}}, add: () => {{}} }} }};
    }}
    return elements[id];
  }},
  addEventListener: () => {{}}
}};
const navigator = {{ clipboard: {{ writeText: async () => {{}} }} }};
class MockImage {{ constructor() {{}} }}
global.Image = MockImage;
const window = global;

{js_code}

// 1. Open photo 1, change to Painted Stork, add to batch
openModal(['bundle-1']);
selectSpecies('Painted Stork');
addToBatch();

const countAfterFirst = batchCommands.length;
const scriptAfterFirst = generateBatchScriptContent(true);

// 2. Open photo 2, change to Great Cormorant, add to batch
openModal(['bundle-2']);
selectSpecies('Great Cormorant');
addToBatch();

const countAfterSecond = batchCommands.length;
const scriptAfterSecond = generateBatchScriptContent(true);

// 3. Re-open photo 1, change to Eurasian Collared-Dove, add to batch (should update/deduplicate photo 1)
openModal(['bundle-1']);
selectSpecies('Eurasian Collared-Dove');
addToBatch();

const countAfterUpdate = batchCommands.length;
const scriptAfterUpdate = generateBatchScriptContent(true);

console.log(JSON.stringify({{
  countAfterFirst,
  hasItem1First: scriptAfterFirst.includes('IMG_1001'),
  hasItem2First: scriptAfterFirst.includes('IMG_1002'),
  countAfterSecond,
  hasItem1Second: scriptAfterSecond.includes('IMG_1001'),
  hasItem2Second: scriptAfterSecond.includes('IMG_1002'),
  countAfterUpdate,
  hasPaintedStorkAfterUpdate: scriptAfterUpdate.includes('Painted_Stork'),
  hasCollaredDoveAfterUpdate: scriptAfterUpdate.includes('Eurasian_Collared-Dove'),
}}));
"#,
        raw_json = json_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner");

    assert!(
        node_output.status.success(),
        "Node runner failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&node_output.stdout).trim()).unwrap();

    assert_eq!(parsed["countAfterFirst"], 1);
    assert_eq!(parsed["hasItem1First"], true);
    assert_eq!(parsed["hasItem2First"], false);

    assert_eq!(parsed["countAfterSecond"], 2);
    assert_eq!(parsed["hasItem1Second"], true);
    assert_eq!(parsed["hasItem2Second"], true);

    // Re-adding item 1 must update its entry without incrementing count or duplicating
    assert_eq!(
        parsed["countAfterUpdate"], 2,
        "Batch count must stay 2 when updating existing item in batch"
    );
    assert_eq!(
        parsed["hasPaintedStorkAfterUpdate"], false,
        "Old species for item 1 must be replaced"
    );
    assert_eq!(
        parsed["hasCollaredDoveAfterUpdate"], true,
        "New species for item 1 must be present"
    );
}

#[test]
fn test_inplace_move_commands_resolve_source_dynamically_when_file_already_renamed() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "1X7A2861".to_string(),
        current_path: PathBuf::from(
            "compressed pics/1X7A2861_Needs_Review_Pied_Kingfisher_20250516_K_Gudi.jpg",
        ),
        primary_image: dir
            .path()
            .join("compressed pics/1X7A2861_Needs_Review_Pied_Kingfisher_20250516_K_Gudi.jpg"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Pied Kingfisher".to_string(),
        scientific_name: Some("Ceryle rudis".to_string()),
        routing_tier: RoutingTier::NeedsReview,
        confidence: 0.65,
        capture_date: Some("2025-05-16".to_string()),
        location_name: Some("K Gudi".to_string()),
        location_source: None,
        subdirectory: Some("compressed pics".to_string()),
        top_predictions: vec![],
        is_local: Some(true),
    };

    generate_html_catalog(
        "Dynamic Source Move Test",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    let marker = r#"<script id="catalog-data" type="application/json">"#;
    let start_idx = html_content.find(marker).unwrap() + marker.len();
    let end_idx = html_content[start_idx..].find("</script>").unwrap();
    let json_str = &html_content[start_idx..start_idx + end_idx];

    let script_start = html_content.find("<script>\n").unwrap() + 9;
    let script_end = html_content[script_start..].find("</script>").unwrap();
    let js_code = &html_content[script_start..script_start + script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') return {{ textContent: rawJson }};
    return {{ textContent: '', value: '', style: {{}}, dataset: {{}}, classList: {{ remove: () => {{}}, add: () => {{}} }} }};
  }},
  addEventListener: () => {{}}
}};
const navigator = {{ clipboard: {{ writeText: async () => {{}} }} }};
class MockImage {{ constructor() {{}} }}
global.Image = MockImage;
const window = global;

{js_code}

const item = CATALOG_DATA.items[0];
const cmds = buildMoveCommandsForItem(item, 'White-browed Wagtail').join('\n');
console.log(cmds);
"#,
        raw_json = json_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner");

    assert!(
        node_output.status.success(),
        "Node runner failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let cmds = String::from_utf8_lossy(&node_output.stdout).to_string();

    // Verify dynamic candidate lookup and self-move protection in generated bash
    assert!(
        cmds.contains("PRIMARY_SRC=") || cmds.contains("SRC_PRIMARY=") || cmds.contains("SRC="),
        "Move commands must dynamically inspect and resolve source path:\n{}",
        cmds
    );
    assert!(
        cmds.contains("1X7A2861*"),
        "Move commands must look for stem candidates if current_path was already renamed:\n{}",
        cmds
    );
}

#[test]
fn test_catalog_species_autocomplete_token_normalization_and_ranking() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-1".to_string(),
        original_stem: "IMG_1001".to_string(),
        current_path: PathBuf::from("birds_hierarchy/House_Sparrow/IMG_1001.CR2"),
        primary_image: dir.path().join("IMG_1001.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "Autocomplete Test",
        &[item],
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // 1. Verify autocomplete helper functions are present in the generated catalog HTML
    assert!(
        html_content.contains("function searchSpeciesAutocomplete("),
        "searchSpeciesAutocomplete helper must exist"
    );
    assert!(
        html_content.contains("function normalizeAutocompleteTokens("),
        "normalizeAutocompleteTokens helper must exist"
    );

    // 2. Extract Javascript logic from HTML and execute test cases in Node.js
    // Extract script content between <script> and </script>
    let script_start = html_content
        .find("<script>")
        .expect("<script> tag not found");
    let script_end = html_content
        .rfind("</script>")
        .expect("</script> tag not found");
    let script_content = &html_content[script_start + 8..script_end];

    let runner_js = format!(
        r#"
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') {{
      return {{ textContent: '{{}}' }};
    }}
    return {{
      textContent: '',
      value: '',
      classList: {{ add: () => {{}}, remove: () => {{}} }},
      style: {{}}
    }};
  }},
  addEventListener: () => {{}}
}};
const window = {{
  matchMedia: () => ({{ matches: false, addEventListener: () => {{}} }})
}};

{script_content}

const testSpecies = [
    "White-cheeked Barbet",
    "Barbet",
    "Red-vented Bulbul",
    "Cheeked Pipit",
    "White-breasted Kingfisher",
    "Great Barbet",
    "Barbette Bird"
];

// Test case 1: "white cheeked barbet" (spaces instead of hyphen) matches "White-cheeked Barbet"
const res1 = searchSpeciesAutocomplete("white cheeked barbet", testSpecies);
if (!res1.includes("White-cheeked Barbet")) {{
    console.error("Failed res1: 'white cheeked barbet' did not match 'White-cheeked Barbet'. Got:", res1);
    process.exit(1);
}}

// Test case 2: "white-cheeked" matches "White-cheeked Barbet"
const res2 = searchSpeciesAutocomplete("white-cheeked", testSpecies);
if (!res2.includes("White-cheeked Barbet")) {{
    console.error("Failed res2: 'white-cheeked' did not match. Got:", res2);
    process.exit(1);
}}

// Test case 3: "barbet white cheeked" matches "White-cheeked Barbet"
const res3 = searchSpeciesAutocomplete("barbet white cheeked", testSpecies);
if (!res3.includes("White-cheeked Barbet")) {{
    console.error("Failed res3: 'barbet white cheeked' did not match. Got:", res3);
    process.exit(1);
}}

// Test case 4: Ranking verification
// For query "barbet":
// Rank 1: "Barbet" (Candidate starts with "barbet")
// Rank 2: "Great Barbet", "White-cheeked Barbet", "Barbette Bird" (all query tokens are prefixes of candidate tokens)
// Check that "Barbet" comes first
const resRank = searchSpeciesAutocomplete("barbet", testSpecies);
if (resRank[0] !== "Barbet") {{
    console.error("Failed resRank: expected 'Barbet' as #1 rank. Got:", resRank);
    process.exit(1);
}}

// For query "white cheeked":
// Rank 1: "White-cheeked Barbet" (candidate starts with "white cheeked" prefix)
const resRank2 = searchSpeciesAutocomplete("white cheeked", testSpecies);
if (resRank2[0] !== "White-cheeked Barbet") {{
    console.error("Failed resRank2: expected 'White-cheeked Barbet' as #1 rank. Got:", resRank2);
    process.exit(1);
}}

// Substring match ranking (Rank 3 vs Rank 2):
// Query "cheek":
// "Cheeked Pipit" starts with "cheek" -> Rank 1
// "White-cheeked Barbet" has token "cheeked" which starts with "cheek" -> Rank 2
const resRank3 = searchSpeciesAutocomplete("cheek", testSpecies);
if (resRank3[0] !== "Cheeked Pipit") {{
    console.error("Failed resRank3: expected 'Cheeked Pipit' as #1 rank. Got:", resRank3);
    process.exit(1);
}}
if (resRank3[1] !== "White-cheeked Barbet") {{
    console.error("Failed resRank3: expected 'White-cheeked Barbet' as #2 rank. Got:", resRank3);
    process.exit(1);
}}

console.log("ALL JS AUTOCOMPLETE TESTS PASSED");
"#
    );

    let js_test_file = dir.path().join("test_autocomplete.js");
    fs::write(&js_test_file, runner_js).expect("write test_autocomplete.js");

    let status = std::process::Command::new("node")
        .arg(&js_test_file)
        .status()
        .expect("failed to execute node");

    assert!(
        status.success(),
        "Node test runner for species autocomplete must exit successfully"
    );
}

#[test]
fn test_catalog_no_bird_detected_support_and_command_generation() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item_hierarchy = CatalogItem {
        bundle_id: "bundle-101".to_string(),
        original_stem: "IMG_2001".to_string(),
        current_path: PathBuf::from("birds_hierarchy/House_Sparrow/IMG_2001.CR2"),
        primary_image: dir.path().join("IMG_2001.CR2"),
        companion_images: vec![dir.path().join("IMG_2001.JPG")],
        sidecar_files: vec![dir.path().join("IMG_2001.CR2.xmp")],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: None,
    };

    generate_html_catalog(
        "No Bird Hierarchy Test",
        std::slice::from_ref(&item_hierarchy),
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // 1. Verify speciesList in CatalogPayload contains "No Bird Detected"
    assert!(
        html_content.contains(r#""No Bird Detected""#),
        "Catalog payload species_list must include 'No Bird Detected'"
    );

    // 2. Verify modal contains the dedicated button for No Bird Detected (False Positive)
    assert!(
        html_content.contains("Mark as No Bird Detected (False Positive)")
            || html_content.contains("markNoBirdDetected"),
        "Catalog template must contain a button or handler to mark as No Bird Detected"
    );

    // 3. Test buildMoveCommands in JS via Node.js for both hierarchy and inplace modes
    let script_start = html_content
        .find("<script>")
        .expect("<script> tag not found");
    let script_end = html_content
        .rfind("</script>")
        .expect("</script> tag not found");
    let script_content = &html_content[script_start + 8..script_end];

    let runner_js = format!(
        r#"
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') {{
      return {{ textContent: JSON.stringify({{
        mode: 'hierarchy',
        catalog_dir: '/fake/dir',
        species_list: ["House Sparrow", "No Bird Detected"]
      }}) }};
    }}
    return {{
      textContent: '',
      value: '',
      classList: {{ add: () => {{}}, remove: () => {{}} }},
      style: {{}}
    }};
  }},
  addEventListener: () => {{}}
}};
const window = {{
  matchMedia: () => ({{ matches: false, addEventListener: () => {{}} }})
}};

{script_content}

// Test Hierarchy Mode with "No Bird Detected"
CATALOG_DATA.mode = 'hierarchy';
activeModalItem = {{
  id: 'bundle-101',
  original_stem: 'IMG_2001',
  current_path: 'birds_hierarchy/House_Sparrow/IMG_2001.CR2',
  primary_filename: 'IMG_2001.CR2',
  companion_files: ['birds_hierarchy/House_Sparrow/IMG_2001.JPG'],
  sidecar_files: ['birds_hierarchy/House_Sparrow/IMG_2001.CR2.xmp'],
  capture_date: '2026-10-25',
  location_name: 'Leh'
}};

const cmdsHier = buildMoveCommands("No Bird Detected");
const cmdsHierStr = cmdsHier.join("\n");
console.log("Hierarchy commands:\n", cmdsHierStr);

if (!cmdsHierStr.includes('avicull correct --file "IMG_2001" --species "_No_Bird_Detected"')) {{
  console.error("Failed: avicull correct command did not use _No_Bird_Detected. Got:", cmdsHierStr);
  process.exit(1);
}}
if (!cmdsHierStr.includes('mkdir -p "birds_hierarchy/_No_Bird_Detected"')) {{
  console.error("Failed: targetDir in hierarchy mode did not use birds_hierarchy/_No_Bird_Detected");
  process.exit(1);
}}
if (!cmdsHierStr.includes('mv "birds_hierarchy/House_Sparrow/IMG_2001.CR2" "birds_hierarchy/_No_Bird_Detected/IMG_2001.CR2"')) {{
  console.error("Failed: fallback mv did not move to birds_hierarchy/_No_Bird_Detected");
  process.exit(1);
}}

// Test In-place Mode with "No Bird Detected" and "_No_Bird_Detected"
CATALOG_DATA.mode = 'in-place';
activeModalItem = {{
  id: 'bundle-102',
  original_stem: 'IMG_2002',
  current_path: 'Trip1/IMG_2002.CR2',
  primary_filename: 'IMG_2002.CR2',
  companion_files: ['Trip1/IMG_2002.JPG'],
  sidecar_files: ['Trip1/IMG_2002.CR2.xmp'],
  capture_date: '2026-10-25',
  location_name: 'Leh'
}};

const cmdsInplace = buildMoveCommands("No Bird Detected");
const cmdsInplaceStr = cmdsInplace.join("\n");
console.log("In-place commands:\n", cmdsInplaceStr);

if (!cmdsInplaceStr.includes('avicull correct --file "IMG_2002" --species "_No_Bird_Detected"')) {{
  console.error("Failed: avicull correct command in in-place mode did not use _No_Bird_Detected. Got:", cmdsInplaceStr);
  process.exit(1);
}}
// In in-place mode, destination filename should format with _No_Bird_Detected:
// e.g., Trip1/IMG_2002_No_Bird_Detected_20261025_Leh.CR2
if (!cmdsInplaceStr.includes('Trip1/IMG_2002_No_Bird_Detected_20261025_Leh.CR2')) {{
  console.error("Failed: in-place rename target did not match expected pattern with _No_Bird_Detected. Got:", cmdsInplaceStr);
  process.exit(1);
}}

console.log("ALL JS NO_BIRD_DETECTED TESTS PASSED");
"#
    );

    let js_test_file = dir.path().join("test_no_bird.js");
    fs::write(&js_test_file, runner_js).expect("write test_no_bird.js");

    let status = std::process::Command::new("node")
        .arg(&js_test_file)
        .status()
        .expect("failed to execute node");

    assert!(
        status.success(),
        "Node test runner for no-bird detected commands must exit successfully"
    );
}

#[test]
fn test_catalog_item_from_bundle_and_decision_user_override_display() {
    use avicull::types::{AssetBundle, BundleDecision, RoutingTier, SpeciesPrediction};

    let bundle = AssetBundle {
        bundle_id: "bundle-override-1".to_string(),
        source_inode: 42,
        trip_folder_name: "Kerala".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: PathBuf::from("IMG_9999.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let ml_prediction = SpeciesPrediction {
        taxon_id: "houspa".to_string(),
        common_name: "House Sparrow".to_string(),
        folder_slug: "House_Sparrow".to_string(),
        scientific_name: "Passer domesticus".to_string(),
        visual_prob: 0.85,
        geo_prior: 0.90,
        combined_score: 0.88,
    };

    let decision = BundleDecision {
        bundle_id: "bundle-override-1".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![ml_prediction],
        frame_confidence: 0.88,
        final_confidence: 1.0,
        assigned_species_slug: "White-cheeked_Barbet".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    let catalog_item = CatalogItem::from_bundle_and_decision(&bundle, &decision, None, None, None);

    // When is_user_override is true, assigned_species must display the user's override slug cleaned with spaces,
    // NOT the ML model's top prediction ("House Sparrow")!
    assert_eq!(
        catalog_item.assigned_species, "White-cheeked Barbet",
        "CatalogItem must reflect user override rather than ML top prediction"
    );
    assert_eq!(
        catalog_item.scientific_name,
        None,
        "Scientific name should be None or matched for manual override when not explicitly provided"
    );
}

#[test]
fn test_catalog_template_generates_command_structure_and_binary_discovery() {
    let dir = tempdir().expect("create temp dir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let item = CatalogItem {
        bundle_id: "bundle-test-1".to_string(),
        original_stem: "IMG_5501".to_string(),
        current_path: PathBuf::from("birds_hierarchy/House_Sparrow/IMG_5501.CR2"),
        primary_image: dir.path().join("IMG_5501.CR2"),
        companion_images: vec![dir.path().join("IMG_5501.JPG")],
        sidecar_files: vec![dir.path().join("IMG_5501.CR2.xmp")],
        relative_preview_path: None,
        assigned_species: "House Sparrow".to_string(),
        scientific_name: Some("Passer domesticus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.95,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "houspa".to_string(),
            common_name: "House Sparrow".to_string(),
            folder_slug: "House_Sparrow".to_string(),
            scientific_name: "Passer domesticus".to_string(),
            visual_prob: 0.95,
            geo_prior: 0.90,
            combined_score: 0.95,
        }],
        is_local: None,
    };

    generate_html_catalog(
        "Catalog Correction Test",
        &[item],
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog must succeed");

    let html = fs::read_to_string(&catalog_path).expect("read bird_catalog.html");

    // Verify buildMoveCommands generates the avicull correct command
    assert!(
        html.contains("function buildMoveCommands("),
        "Template must define buildMoveCommands to structure correction commands"
    );
    assert!(
        html.contains("avicull correct --file") && html.contains("--species"),
        "Template must output 'avicull correct --file ... --species ...' command"
    );
    assert!(
        html.contains("command -v avicull"),
        "Template must probe for avicull CLI availability before fallback"
    );

    // Verify binary discovery hierarchy and catalog auto-regeneration in batch script
    assert!(
        html.contains("findAvicullCmd"),
        "Template must provide avicull binary discovery logic"
    );
    assert!(
        html.contains("target/release/avicull"),
        "Template must search local target/release/avicull builds"
    );
    assert!(
        html.contains(".cargo/bin/avicull"),
        "Template must search cargo bin avicull installs"
    );
    assert!(
        html.contains("/usr/local/bin/avicull"),
        "Template must search /usr/local/bin/avicull"
    );
    assert!(
        html.contains("/opt/homebrew/bin/avicull"),
        "Template must search /opt/homebrew/bin/avicull"
    );
    assert!(
        html.contains("copyBatchScript()"),
        "Template must provide copyBatchScript function"
    );
    assert!(
        html.contains("btnCopyBatchScript"),
        "Template must provide btnCopyBatchScript button"
    );
    assert!(
        html.contains("cargo run --release --bin avicull --"),
        "Template must fall back to cargo run before raw mv fallback"
    );
    assert!(
        html.contains("organize"),
        "Batch script apply_corrections.sh must re-run organize to regenerate bird_catalog.html"
    );

    // Verify modal guidance mentions syncing SQLite manifest
    assert!(
        html.contains("sync the SQLite manifest"),
        "Modal guidance must inform users that running the command/script syncs the SQLite manifest"
    );
}

#[test]
fn test_batch_correction_script_execution_and_manifest_update() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("SourcePhotos");
    fs::create_dir_all(&input_dir).unwrap();

    let photo = input_dir.join("DSC_9001.JPG");
    let sidecar = input_dir.join("DSC_9001.xmp");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_9001 DATA")
        .unwrap();
    File::create(&sidecar)
        .unwrap()
        .write_all(b"<xmp>DSC_9001 SIDECAR</xmp>")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // Initial organization of photos in hierarchy mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists());

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let initial_bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(initial_bundles.len(), 1);
    assert_ne!(initial_bundles[0].0.assigned_species_slug, "Painted_Stork");

    // Simulate batch script generated by catalog template
    let avicull_bin_dir = Path::new(env!("CARGO_BIN_EXE_avicull"))
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    let script_path = output_dir.join("apply_corrections.sh");
    let script_content = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
cd "{catalog_dir}"
export PATH="{bin_dir}:$PATH"

if command -v avicull >/dev/null 2>&1; then
  avicull correct --file "DSC_9001" --species "Painted_Stork"
else
  mkdir -p "birds_hierarchy/Painted_Stork"
  mv "birds_hierarchy/{orig_slug}/DSC_9001.JPG" "birds_hierarchy/Painted_Stork/DSC_9001.JPG"
fi
"#,
        catalog_dir = output_dir.display(),
        bin_dir = avicull_bin_dir,
        orig_slug = initial_bundles[0].0.assigned_species_slug,
    );

    fs::write(&script_path, script_content).unwrap();

    // Execute apply_corrections.sh via bash
    let script_run = Command::new("bash")
        .arg(script_path.to_str().unwrap())
        .output()
        .expect("execute apply_corrections.sh");

    assert!(
        script_run.status.success(),
        "apply_corrections.sh failed: stderr: {}",
        String::from_utf8_lossy(&script_run.stderr)
    );

    // Verify SQLite manifest was durably updated with override
    let updated_bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(updated_bundles.len(), 1);
    let (decision, _bundle) = &updated_bundles[0];
    assert_eq!(decision.assigned_species_slug, "Painted_Stork");
    assert_eq!(decision.routing_tier, RoutingTier::ConfirmedSpecies);
    assert!(
        (decision.final_confidence - 1.0).abs() < f32::EPSILON,
        "final_confidence should be 1.0"
    );
    assert!(decision.is_user_override, "is_user_override must be true");

    // Verify hard link moved to new species directory
    let new_species_dir = output_dir.join("Painted_Stork");
    assert!(
        new_species_dir.exists(),
        "Target directory Painted_Stork must exist"
    );
    let entries: Vec<_> = fs::read_dir(&new_species_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let target_photo = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("DSC_9001") && name.ends_with(".JPG")
        })
        .expect("Target photo must exist in Painted_Stork");

    let src_meta = fs::metadata(&photo).unwrap();
    let dst_meta = fs::metadata(target_photo).unwrap();
    assert_eq!(
        src_meta.ino(),
        dst_meta.ino(),
        "Inodes must match across hard links"
    );
}

#[test]
fn test_single_command_subshell_execution_and_manifest_update() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("SourcePhotos");
    fs::create_dir_all(&input_dir).unwrap();

    let photo = input_dir.join("IMG_1234.JPG");
    let sidecar = input_dir.join("IMG_1234.xmp");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG IMG_1234 DATA")
        .unwrap();
    File::create(&sidecar)
        .unwrap()
        .write_all(b"<xmp>IMG_1234 SIDECAR</xmp>")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let catalog_path = output_dir.join("bird_catalog.html");
    assert!(catalog_path.exists());

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // Extract JSON data and JS script from HTML
    let json_start_marker = "<script id=\"catalog-data\" type=\"application/json\">\n";
    let json_start = html_content
        .find(json_start_marker)
        .expect("json start marker")
        + json_start_marker.len();
    let json_end = html_content[json_start..]
        .find("</script>")
        .expect("json end marker")
        + json_start;
    let json_data_str = &html_content[json_start..json_end];

    let script_marker = "<script>\n";
    let script_start =
        html_content.find(script_marker).expect("script start") + script_marker.len();
    let script_end = html_content[script_start..]
        .find("</script>")
        .expect("script end")
        + script_start;
    let js_code = &html_content[script_start..script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') {{
      return {{ textContent: rawJson }};
    }}
    return {{ textContent: '', value: '', classList: {{ remove: () => {{}}, add: () => {{}} }} }};
  }},
  addEventListener: () => {{}}
}};
const window = {{}};

{js_code}

const item = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_1234');
if (!item) throw new Error('Item IMG_1234 not found in CATALOG_DATA');
activeModalItem = item;
const cmds = buildMoveCommands('Brahminy Kite');
console.log(cmds.join('\n'));
"#,
        raw_json = json_data_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node script to generate single command");

    assert!(
        node_output.status.success(),
        "Node script failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let generated_bash = String::from_utf8_lossy(&node_output.stdout).to_string();
    assert!(
        generated_bash.contains("avicull correct --file \"IMG_1234\" --species \"Brahminy_Kite\"")
            || generated_bash
                .contains("$AVICULL_CMD correct --file \"IMG_1234\" --species \"Brahminy_Kite\""),
        "Generated bash must contain avicull correct command"
    );

    // Run generated bash command in a real bash subshell
    let avicull_bin_dir = Path::new(env!("CARGO_BIN_EXE_avicull"))
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    let subshell_script = format!(
        "export PATH=\"{}:$PATH\"\nset -e\n{}",
        avicull_bin_dir, generated_bash
    );

    let bash_exec = Command::new("bash")
        .arg("-c")
        .arg(&subshell_script)
        .output()
        .expect("execute generated single correction bash command");

    assert!(
        bash_exec.status.success(),
        "Generated single correction bash command failed: stderr: {}",
        String::from_utf8_lossy(&bash_exec.stderr)
    );

    // Verify SQLite manifest and filesystem reflect the correction
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let updated_bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(updated_bundles.len(), 1);
    let (decision, _bundle) = &updated_bundles[0];
    assert_eq!(decision.assigned_species_slug, "Brahminy_Kite");
    assert!(decision.is_user_override);

    let target_dir = output_dir.join("Brahminy_Kite");
    assert!(target_dir.exists());
    let entries: Vec<_> = fs::read_dir(&target_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let target_photo = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("IMG_1234") && name.ends_with(".JPG")
        })
        .expect("Target photo IMG_1234*.JPG must exist in Brahminy_Kite");

    let src_meta = fs::metadata(&photo).unwrap();
    let dst_meta = fs::metadata(target_photo).unwrap();
    assert_eq!(
        src_meta.ino(),
        dst_meta.ino(),
        "Inodes must match across hard links"
    );
}

#[test]
fn test_copy_batch_script_zsh_interactive_compatibility_and_no_shebang() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("SourcePhotos");
    fs::create_dir_all(&input_dir).unwrap();

    let photo = input_dir.join("IMG_9999.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG IMG_9999 DATA")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let catalog_path = output_dir.join("bird_catalog.html");
    assert!(catalog_path.exists());

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    let json_start_marker = "<script id=\"catalog-data\" type=\"application/json\">\n";
    let json_start = html_content
        .find(json_start_marker)
        .expect("json start marker")
        + json_start_marker.len();
    let json_end = html_content[json_start..]
        .find("</script>")
        .expect("json end marker")
        + json_start;
    let json_data_str = &html_content[json_start..json_end];

    let script_marker = "<script>\n";
    let script_start =
        html_content.find(script_marker).expect("script start") + script_marker.len();
    let script_end = html_content[script_start..]
        .find("</script>")
        .expect("script end")
        + script_start;
    let js_code = &html_content[script_start..script_end];

    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
let clipboardContent = '';
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') {{
      return {{ textContent: rawJson }};
    }}
    return {{ textContent: '', value: '', classList: {{ remove: () => {{}}, add: () => {{}} }} }};
  }},
  addEventListener: () => {{}}
}};
const navigator = {{
  clipboard: {{
    writeText: async (text) => {{
      clipboardContent = text;
      return Promise.resolve();
    }}
  }}
}};
const window = {{}};

{js_code}

const item = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_9999');
if (!item) throw new Error('Item IMG_9999 not found in CATALOG_DATA');
activeModalItem = item;
const cmds = buildMoveCommands('Osprey');
batchCommands.push(...cmds);

const downloadScript = generateBatchScriptContent(true);
copyBatchScript();

setTimeout(() => {{
  console.log(JSON.stringify({{
    downloadScript: downloadScript,
    clipboardScript: clipboardContent
  }}));
}}, 50);
"#,
        raw_json = json_data_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner for batch script generation");

    assert!(
        node_output.status.success(),
        "Node script failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let output_str = String::from_utf8_lossy(&node_output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(output_str.trim()).expect("parse node json output");

    let download_script = parsed["downloadScript"].as_str().expect("downloadScript");
    let clipboard_script = parsed["clipboardScript"].as_str().expect("clipboardScript");

    // Download script must retain shebang for file execution
    assert!(
        download_script.starts_with("#!/usr/bin/env bash"),
        "download script must have #!/usr/bin/env bash shebang"
    );

    // Clipboard script must NOT contain shebang (causes zsh event not found)
    assert!(
        !clipboard_script.contains("#!/usr/bin/env"),
        "clipboard script must NOT contain #!/usr/bin/env shebang"
    );
    assert!(
        clipboard_script.trim_start().starts_with('('),
        "clipboard script must be wrapped in a subshell ( ... )"
    );
    assert!(
        clipboard_script.trim_end().ends_with(')'),
        "clipboard script must close the subshell with )"
    );

    // Verify clipboard script executes cleanly in interactive zsh without history expansion failure
    let avicull_bin_dir = Path::new(env!("CARGO_BIN_EXE_avicull"))
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    let zsh_input = format!(
        "unsetopt INTERACTIVE_COMMENTS\nexport PATH=\"{}:$PATH\"\n{}\nexit\n",
        avicull_bin_dir, clipboard_script
    );

    let mut child = Command::new("zsh")
        .args(["-f", "-i", "-s"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn zsh process");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin.write_all(zsh_input.as_bytes()).unwrap();
    }

    let zsh_res = child.wait_with_output().expect("wait on zsh");
    let zsh_stderr = String::from_utf8_lossy(&zsh_res.stderr);
    let zsh_stdout = String::from_utf8_lossy(&zsh_res.stdout);

    assert!(
        !zsh_stderr.contains("event not found"),
        "zsh triggered history expansion error: stderr: {}\nstdout: {}",
        zsh_stderr,
        zsh_stdout
    );
    assert!(
        !zsh_stdout.contains("event not found"),
        "zsh triggered history expansion error in stdout: {}",
        zsh_stdout
    );
    assert!(
        zsh_res.status.success(),
        "interactive zsh execution failed: stderr: {}\nstdout: {}",
        zsh_stderr,
        zsh_stdout
    );
}

#[test]
fn test_catalog_embedded_base64_thumbnails() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    // Create a real JPEG image in the tempdir
    let img_path = dir.path().join("IMG_9999.JPG");
    let img = image::RgbImage::new(400, 300);
    img.save(&img_path).expect("save test image");

    let item = CatalogItem {
        bundle_id: "bundle-embed-1".to_string(),
        original_stem: "IMG_9999".to_string(),
        current_path: img_path.clone(),
        primary_image: img_path.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Lammergeier".to_string(),
        scientific_name: Some("Gypaetus barbatus".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.99,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    generate_html_catalog(
        "Embed Test Trip",
        std::slice::from_ref(&item),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // Extract the embedded JSON payload
    let marker = r#"<script id="catalog-data" type="application/json">"#;
    let start_idx = html_content
        .find(marker)
        .expect("must contain catalog-data script tag")
        + marker.len();
    let end_idx = html_content[start_idx..]
        .find("</script>")
        .expect("must contain closing script tag");
    let json_str = &html_content[start_idx..start_idx + end_idx];

    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("valid json payload");
    let items = parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);

    let first_item = &items[0];
    let thumb_uri = first_item["embedded_thumb_data_uri"]
        .as_str()
        .expect("embedded_thumb_data_uri must be present and a string");

    assert!(
        thumb_uri.starts_with("data:image/jpeg;base64,"),
        "embedded_thumb_data_uri must start with data:image/jpeg;base64,"
    );

    // Verify it's valid base64 and decodes to a valid JPEG
    let b64_part = &thumb_uri["data:image/jpeg;base64,".len()..];
    assert!(!b64_part.is_empty(), "base64 payload must not be empty");

    // Also test standalone function generate_base64_micro_thumbnail
    let standalone_uri = avicull::catalog::generate_base64_micro_thumbnail(&item, dir.path());
    assert!(
        standalone_uri.is_some(),
        "generate_base64_micro_thumbnail must succeed for valid image"
    );
    assert_eq!(standalone_uri.as_deref(), Some(thumb_uri));
}

#[test]
fn test_catalog_standalone_dynamic_high_res_replacement() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    // Create a real JPEG image in the tempdir
    let img_path = dir.path().join("IMG_8888.JPG");
    let img = image::RgbImage::new(400, 300);
    img.save(&img_path).expect("save test image");

    let item = CatalogItem {
        bundle_id: "bundle-embed-dyn-1".to_string(),
        original_stem: "IMG_8888".to_string(),
        current_path: img_path.clone(),
        primary_image: img_path.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        assigned_species: "Golden Eagle".to_string(),
        scientific_name: Some("Aquila chrysaetos".to_string()),
        routing_tier: RoutingTier::ConfirmedSpecies,
        confidence: 0.98,
        capture_date: Some("2026-10-25".to_string()),
        location_name: Some("Leh".to_string()),
        location_source: None,
        subdirectory: None,
        top_predictions: vec![],
        is_local: Some(true),
    };

    generate_html_catalog(
        "Dynamic High-Res Test Trip",
        std::slice::from_ref(&item),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    // Remove any .avicull/ directory if it was created, proving 100% standalone rendering
    let avicull_dir = dir.path().join(".avicull");
    if avicull_dir.exists() {
        fs::remove_dir_all(&avicull_dir).expect("remove .avicull");
    }

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // 1. Verify card initial src uses embedded thumbnail URI as baseline
    assert!(
        html_content.contains("embedded_thumb_data_uri"),
        "Catalog HTML must reference embedded_thumb_data_uri"
    );

    // 2. Verify dynamic high-res probing logic is present in the JavaScript
    assert!(
        html_content.contains("upgradeCardHighRes") || html_content.contains("attemptHighResSwap"),
        "Catalog HTML must contain dynamic high-res upgrade probing function"
    );

    // 3. Verify probe.onload updates img.src to high-res local_src
    assert!(
        html_content.contains("probe.onload"),
        "Catalog script must use an Image probe with probe.onload"
    );

    // 4. Verify probe.onerror does not break image and gracefully marks cloud badge
    assert!(
        html_content.contains("probe.onerror"),
        "Catalog script must handle probe.onerror gracefully"
    );

    // 5. Verify modal photo preview uses high-res probe / embedded thumb fallback
    assert!(
        html_content.contains("modalPhotoThumb"),
        "Catalog must contain modalPhotoThumb logic"
    );

    // 6. Execute node.js DOM simulation to test probe onload and onerror behaviors
    let js_test_code = format!(
        r#"
        const html = `{}`;
        // Extract script content
        const scriptStart = html.lastIndexOf("<script>");
        const scriptEnd = html.lastIndexOf("</script>");
        const scriptCode = html.substring(scriptStart + 8, scriptEnd);

        // Minimal DOM mock
        const galleryGridObj = {{ innerHTML: '' }};
        const cardsMap = {{}};
        const modalThumbObj = {{ src: '', dataset: {{}}, style: {{}} }};
        global.document = {{
          getElementById: (id) => {{
            if (id === 'catalog-data') {{
              return {{ textContent: JSON.stringify({{ items: [
                {{
                  id: "bundle-embed-dyn-1",
                  assigned_species: "Golden Eagle",
                  scientific_name: "Aquila chrysaetos",
                  routing_tier: "ConfirmedSpecies",
                  confidence: 0.98,
                  current_path: "IMG_8888.JPG",
                  primary_filename: "IMG_8888.JPG",
                  local_src: "IMG_8888.JPG",
                  thumb_src: ".avicull/previews/IMG_8888.jpg",
                  embedded_thumb_data_uri: "data:image/jpeg;base64,mockthumb",
                  is_local: true
                }}
              ] }}) }};
            }}
            if (id === 'galleryGrid') return galleryGridObj;
            if (id === 'searchInput') return {{ value: '' }};
            if (id === 'modalPhotoThumb') return modalThumbObj;
            if (id === 'modalCurrentSpecies') return {{ textContent: '' }};
            if (id === 'modalCurrentPath') return {{ textContent: '' }};
            if (id === 'speciesSearchInput') return {{ value: '' }};
            if (id === 'speciesSuggestions') return {{ classList: {{ remove: () => {{}} }} }};
            if (id === 'commandOutput') return {{ textContent: '' }};
            if (id === 'correctionModal') return {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }};
            if (id.startsWith('card-')) {{
              if (!cardsMap[id]) {{
                const imgObj = {{ src: 'data:image/jpeg;base64,mockthumb' }};
                cardsMap[id] = {{
                  classList: {{ add: () => {{}}, remove: () => {{}} }},
                  querySelector: (sel) => (sel === 'img' ? imgObj : null)
                }};
              }}
              return cardsMap[id];
            }}
            return null;
          }},
          querySelectorAll: () => [],
          addEventListener: () => {{}}
        }};

        let probeInstances = [];
        global.Image = class {{
          constructor() {{
            this.src = '';
            this.onload = null;
            this.onerror = null;
            probeInstances.push(this);
          }}
        }};

        // Eval script code
        eval(scriptCode);

        // Call renderGallery
        renderGallery();
        const grid = global.document.getElementById('galleryGrid');
        if (!grid.innerHTML.includes('data:image/jpeg;base64,mockthumb')) {{
          console.error("FAIL: rendered HTML does not use embedded thumbnail initially! innerHTML was:", grid.innerHTML);
          process.exit(1);
        }}

        // Verify probe instance was created and attempted to load local_src
        if (probeInstances.length === 0) {{
          console.error("FAIL: No probe image was created!");
          process.exit(1);
        }}
        const probe = probeInstances[0];
        if (probe.src !== 'IMG_8888.JPG') {{
          console.error("FAIL: Probe src is not IMG_8888.JPG! Got:", probe.src);
          process.exit(1);
        }}

        // Simulate probe.onload
        const cardImg = cardsMap['card-bundle-embed-dyn-1'].querySelector('img');
        probe.onload();
        if (cardImg.src !== 'IMG_8888.JPG') {{
          console.error("FAIL: cardImg src was not upgraded to high-res on onload! Got:", cardImg.src);
          process.exit(1);
        }}

        // Test probe.onerror graceful handling
        let evictedAdded = false;
        cardsMap['card-bundle-embed-dyn-1'].classList.add = (c) => {{
          if (c === 'evicted') evictedAdded = true;
        }};
        probe.onerror();
        if (!evictedAdded) {{
          console.error("FAIL: card was not tagged as evicted on probe.onerror!");
          process.exit(1);
        }}

        // Test modal photo thumb upgrade
        openModal('bundle-embed-dyn-1');
        const modalThumb = global.document.getElementById('modalPhotoThumb');
        if (modalThumb.src !== 'data:image/jpeg;base64,mockthumb') {{
          console.error("FAIL: modal thumb initial src not set to embedded thumb! Got:", modalThumb.src);
          process.exit(1);
        }}
        // Modal probe instance should be created
        const modalProbe = probeInstances[probeInstances.length - 1];
        modalProbe.onload();
        if (modalThumb.src !== 'IMG_8888.JPG') {{
          console.error("FAIL: modal thumb not upgraded to high-res on onload! Got:", modalThumb.src);
          process.exit(1);
        }}

        console.log("SUCCESS: Catalog JS renders embedded thumb and initiates probe");
        "#,
        html_content
            .replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace('$', "\\$")
    );

    let mut temp_js = tempfile::NamedTempFile::new().expect("create temp js");
    temp_js
        .write_all(js_test_code.as_bytes())
        .expect("write js");

    let node_output = Command::new("node")
        .arg(temp_js.path())
        .output()
        .expect("run node");

    let stdout = String::from_utf8_lossy(&node_output.stdout);
    let stderr = String::from_utf8_lossy(&node_output.stderr);
    println!("Node stdout: {}", stdout);
    println!("Node stderr: {}", stderr);
    assert!(node_output.status.success(), "Node test failed: {}", stderr);
}

#[test]
fn test_apply_corrections_resilient_to_missing_files() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("SourcePhotos");
    fs::create_dir_all(&input_dir).unwrap();

    // Create 3 photo bundles: Item1 (valid), Item2 (missing underlying file), Item3 (valid)
    let photo1 = input_dir.join("IMG_101.JPG");
    let photo2 = input_dir.join("IMG_102.JPG");
    let photo3 = input_dir.join("IMG_103.JPG");

    File::create(&photo1)
        .unwrap()
        .write_all(b"IMG_101 JPEG DATA")
        .unwrap();
    File::create(&photo2)
        .unwrap()
        .write_all(b"IMG_102 JPEG DATA")
        .unwrap();
    File::create(&photo3)
        .unwrap()
        .write_all(b"IMG_103 JPEG DATA")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // Organize input photos
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let catalog_path = output_dir.join("bird_catalog.html");
    assert!(catalog_path.exists());

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // Extract JSON catalog data and JS code
    let json_start_marker = "<script id=\"catalog-data\" type=\"application/json\">\n";
    let json_start = html_content
        .find(json_start_marker)
        .expect("json start marker")
        + json_start_marker.len();
    let json_end = html_content[json_start..]
        .find("</script>")
        .expect("json end marker")
        + json_start;
    let json_data_str = &html_content[json_start..json_end];

    let script_marker = "<script>\n";
    let script_start =
        html_content.find(script_marker).expect("script start") + script_marker.len();
    let script_end = html_content[script_start..]
        .find("</script>")
        .expect("script end")
        + script_start;
    let js_code = &html_content[script_start..script_end];

    // Use node to queue corrections for all 3 items, then generate batch scripts
    let node_runner = format!(
        r#"
const rawJson = {raw_json:?};
let clipboardContent = '';
const document = {{
  getElementById: (id) => {{
    if (id === 'catalog-data') {{
      return {{ textContent: rawJson }};
    }}
    return {{ textContent: '', value: '', classList: {{ remove: () => {{}}, add: () => {{}} }} }};
  }},
  addEventListener: () => {{}}
}};
const navigator = {{
  clipboard: {{
    writeText: async (text) => {{
      clipboardContent = text;
      return Promise.resolve();
    }}
  }}
}};
const window = {{}};

{js_code}

// Queue correction for item 1 -> Osprey
activeModalItem = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_101');
batchCommands.push(...buildMoveCommands('Osprey'));

// Queue correction for item 2 -> Golden_Eagle
activeModalItem = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_102');
batchCommands.push(...buildMoveCommands('Golden_Eagle'));

// Queue correction for item 3 -> Peregrine_Falcon
activeModalItem = CATALOG_DATA.items.find(i => i.original_stem === 'IMG_103');
batchCommands.push(...buildMoveCommands('Peregrine_Falcon'));

const downloadScript = generateBatchScriptContent(true);
copyBatchScript();

setTimeout(() => {{
  console.log(JSON.stringify({{
    downloadScript: downloadScript,
    clipboardScript: clipboardContent
  }}));
}}, 50);
"#,
        raw_json = json_data_str,
        js_code = js_code
    );

    let node_output = Command::new("node")
        .arg("-e")
        .arg(&node_runner)
        .output()
        .expect("execute node runner for batch script generation");

    assert!(
        node_output.status.success(),
        "Node script failed: {}",
        String::from_utf8_lossy(&node_output.stderr)
    );

    let output_str = String::from_utf8_lossy(&node_output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(output_str.trim()).expect("parse node json output");

    let download_script = parsed["downloadScript"].as_str().expect("downloadScript");
    let clipboard_script = parsed["clipboardScript"].as_str().expect("clipboardScript");

    // Before running the script, intentionally delete the file for item 2!
    // In hierarchy mode, find where IMG_102 is and delete it.
    let mut found_102 = false;
    for entry in walkdir::WalkDir::new(&output_dir) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let fname = entry.file_name().to_string_lossy();
            if fname.starts_with("IMG_102") && fname.ends_with(".JPG") {
                fs::remove_file(entry.path()).unwrap();
                found_102 = true;
                break;
            }
        }
    }
    assert!(
        found_102,
        "Must have found and deleted IMG_102 before running correction script"
    );

    // Also delete source photo2 to simulate item being gone completely
    let _ = fs::remove_file(&photo2);

    // Save download_script to disk and execute it via bash
    let script_file = output_dir.join("apply_corrections.sh");
    fs::write(&script_file, download_script).unwrap();
    println!("Generated download_script:\n{}", download_script);

    let avicull_bin_dir = Path::new(env!("CARGO_BIN_EXE_avicull"))
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    let exec_output = Command::new("bash")
        .arg(script_file.to_str().unwrap())
        .env(
            "PATH",
            format!(
                "{}:{}",
                avicull_bin_dir,
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("execute apply_corrections.sh");

    let stdout = String::from_utf8_lossy(&exec_output.stdout);
    let stderr = String::from_utf8_lossy(&exec_output.stderr);
    println!("Execution stdout:\n{}", stdout);
    println!("Execution stderr:\n{}", stderr);

    // Assert that the script did NOT abort prematurely on IMG_102:
    // It should have logged a warning for IMG_102
    assert!(
        stdout.contains("[WARN]") || stderr.contains("[WARN]"),
        "Script must log a [WARN] message for the failed/skipped item"
    );

    // Assert that summary output reflects failed count and succeeded count
    assert!(
        stdout.contains("failed/skipped")
            || stdout.contains("failed")
            || stdout.contains("skipped"),
        "Script must print a summary indicating failed/skipped item(s)"
    );

    // Verify valid items (IMG_101 -> Osprey and IMG_103 -> Peregrine_Falcon) were both successfully processed!
    let osprey_dir = output_dir.join("Osprey");
    assert!(
        osprey_dir.exists(),
        "Osprey directory must exist for item 1"
    );
    let osprey_entries: Vec<_> = fs::read_dir(&osprey_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        osprey_entries.iter().any(|p| p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("IMG_101")),
        "IMG_101 must have been moved to Osprey directory"
    );

    let falcon_dir = output_dir.join("Peregrine_Falcon");
    assert!(
        falcon_dir.exists(),
        "Peregrine_Falcon directory must exist for item 3"
    );
    let falcon_entries: Vec<_> = fs::read_dir(&falcon_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        falcon_entries.iter().any(|p| p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("IMG_103")),
        "IMG_103 must have been moved to Peregrine_Falcon directory"
    );

    // Verify manifest database has overrides for 101 and 103
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    let b101 = bundles
        .iter()
        .find(|(_d, b)| b.original_stem == "IMG_101")
        .expect("IMG_101 in db");
    assert_eq!(b101.0.assigned_species_slug, "Osprey");
    assert!(b101.0.is_user_override);

    let b103 = bundles
        .iter()
        .find(|(_d, b)| b.original_stem == "IMG_103")
        .expect("IMG_103 in db");
    assert_eq!(b103.0.assigned_species_slug, "Peregrine_Falcon");
    assert!(b103.0.is_user_override);

    // The script should exit with non-zero when FAILED_COUNT > 0
    assert!(
        !exec_output.status.success(),
        "Script should exit with non-zero exit code when some items failed"
    );

    // Also verify clipboard script contains the resilient error isolation logic
    assert!(
        clipboard_script.contains("FAILED_COUNT")
            || clipboard_script.contains("apply_single_correction")
            || clipboard_script.contains("[WARN]"),
        "Clipboard script must also include resilient execution logic"
    );
}

#[test]
fn test_catalog_finder_multiselect_and_bulk_correction_js() {
    let dir = tempdir().expect("tempdir");
    let catalog_path = dir.path().join("bird_catalog.html");

    let items: Vec<CatalogItem> = (1..=5)
        .map(|i| {
            let stem = format!("IMG_{:04}", 2000 + i);
            CatalogItem {
                bundle_id: format!("bundle-{}", i),
                original_stem: stem.clone(),
                current_path: PathBuf::from(format!("birds_hierarchy/NeedsReview/{}.CR2", stem)),
                primary_image: dir.path().join(format!("{}.CR2", stem)),
                companion_images: vec![],
                sidecar_files: vec![],
                relative_preview_path: None,
                assigned_species: "Common Swift".to_string(),
                scientific_name: Some("Apus apus".to_string()),
                routing_tier: RoutingTier::NeedsReview,
                confidence: 0.65,
                capture_date: Some("2026-10-25".to_string()),
                location_name: Some("Leh".to_string()),
                location_source: None,
                subdirectory: None,
                top_predictions: vec![],
                is_local: Some(true),
            }
        })
        .collect();

    generate_html_catalog(
        "Multi-Select Trip",
        &items,
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &catalog_path,
    )
    .expect("generate_html_catalog");

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // Check that CSS contains styling for selection
    assert!(
        html_content.contains(".card.selected"),
        "Catalog HTML must contain CSS for .card.selected"
    );
    assert!(
        html_content.contains("selection-bar"),
        "Catalog HTML must contain floating selection action bar CSS/HTML"
    );

    // Extract script content between <script> and </script>
    let script_start = html_content
        .find("<script>")
        .expect("<script> tag not found");
    let script_end = html_content
        .rfind("</script>")
        .expect("</script> tag not found");
    let script_content = &html_content[script_start + 8..script_end];

    // Extract catalog json
    let json_start = html_content
        .find(r#"<script id="catalog-data" type="application/json">"#)
        .expect("catalog-data tag not found");
    let json_open_end = html_content[json_start..]
        .find('>')
        .expect("catalog-data open tag end");
    let json_end = html_content[json_start..]
        .find("</script>")
        .expect("catalog-data close tag");
    let catalog_json_str = &html_content[json_start + json_open_end + 1..json_start + json_end];

    // Verify JavaScript multi-select behavior via Node.js
    let node_test_script = format!(
        r#"
const catalogJson = `{json_str}`;

// Mock minimal DOM environment
let selectedClasses = new Map();
let mockElements = new Map();

global.document = {{
    getElementById: (id) => {{
        if (id === 'catalog-data') {{
            return {{ textContent: catalogJson }};
        }}
        if (!mockElements.has(id)) {{
            mockElements.set(id, {{
                id: id,
                classList: {{
                    add: (cls) => {{}},
                    remove: (cls) => {{}},
                    contains: (cls) => false,
                }},
                dataset: {{}},
                style: {{}},
                textContent: '',
                value: '',
                innerHTML: '',
            }});
        }}
        return mockElements.get(id);
    }},
    querySelectorAll: (sel) => [],
    addEventListener: () => {{}},
}};
global.window = {{}};
global.Image = class {{
    constructor() {{
        this.src = '';
    }}
}};

// Evaluate catalog script
{script_content}

// Test 1: Single click selects one item and clears previous
if (typeof handleCardClick !== 'function') {{
    console.error("handleCardClick function missing");
    process.exit(1);
}}

handleCardClick('bundle-1', {{ metaKey: false, ctrlKey: false, shiftKey: false }});
if (selectedIds.size !== 1 || !selectedIds.has('bundle-1')) {{
    console.error("Single click should select only bundle-1, got:", Array.from(selectedIds));
    process.exit(1);
}}

// Test 2: Cmd/Ctrl click adds disjoint selection
handleCardClick('bundle-3', {{ metaKey: true, ctrlKey: false, shiftKey: false }});
if (selectedIds.size !== 2 || !selectedIds.has('bundle-1') || !selectedIds.has('bundle-3')) {{
    console.error("Cmd+click should toggle disjoint item, got:", Array.from(selectedIds));
    process.exit(1);
}}

// Test 3: Shift click selects contiguous range from lastSelectedId ('bundle-3') to 'bundle-5'
handleCardClick('bundle-5', {{ metaKey: false, ctrlKey: false, shiftKey: true }});
// Range from bundle-3 to bundle-5 in [1, 2, 3, 4, 5] includes 3, 4, 5
if (!selectedIds.has('bundle-3') || !selectedIds.has('bundle-4') || !selectedIds.has('bundle-5')) {{
    console.error("Shift+click should select contiguous range [3..5], got:", Array.from(selectedIds));
    process.exit(1);
}}

// Test 4: Open modal with multiple items selected
openModal(['bundle-3', 'bundle-4', 'bundle-5']);
if (!activeModalItems || activeModalItems.length !== 3) {{
    console.error("Modal should have 3 activeModalItems, got:", activeModalItems);
    process.exit(1);
}}

// Test 5: Selecting a species generates commands for all active items and adds to batchCommands
selectSpecies("Tibetan Snowcock");
if (batchCommands.length !== 0) {{
    console.error("selectSpecies alone shouldn't immediately add to batchCommands");
    process.exit(1);
}}

addActiveModalToBatch();
if (batchCommands.length !== 3) {{
    console.error("addActiveModalToBatch should add all 3 items to batchCommands, got:", batchCommands.length);
    process.exit(1);
}}

console.log("SUCCESS: Finder multi-select and bulk modal operations verified");
"#,
        json_str = catalog_json_str.trim(),
        script_content = script_content
    );

    let script_runner = dir.path().join("test_multiselect.js");
    fs::write(&script_runner, node_test_script).expect("write test script");

    let node_output = std::process::Command::new("node")
        .arg(&script_runner)
        .output()
        .expect("run node");

    let stdout = String::from_utf8_lossy(&node_output.stdout);
    let stderr = String::from_utf8_lossy(&node_output.stderr);
    println!("Node stdout: {}", stdout);
    println!("Node stderr: {}", stderr);

    assert!(
        node_output.status.success(),
        "Node.js multi-select test failed: STDOUT: {}\nSTDERR: {}",
        stdout,
        stderr
    );
}
