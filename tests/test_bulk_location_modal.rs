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

#[test]
fn test_bulk_location_js_execution_in_node() {
    let dir = tempdir().unwrap();
    let items = vec![
        item_with_subdir("Leh Valley"),
        item_with_subdir("Leh Valley/Shey Marshes"),
    ];
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();

    let json_start = html
        .find(r#"<script id="catalog-data" type="application/json">"#)
        .expect("json script start");
    let after_start = &html[json_start..];
    let data_start = after_start.find('>').expect("close tag") + 1;
    let data_end = after_start.find("</script>").expect("script end");
    let embedded_json = after_start[data_start..data_end].trim();

    let script_start = html
        .find("<script>\n  let CATALOG_DATA")
        .expect("script start not found");
    let script_end = html.rfind("</script>").expect("script end not found");
    let js_code = &html[script_start + 8..script_end];

    let runner_js = format!(
        r#"
const domElements = {{
  "catalog-data": {{ textContent: String.raw`{embedded_json}` }},
  batchCount: {{ textContent: "0" }},
  batchCountCopy: {{ textContent: "0" }},
  locationModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  locationSinglePanel: {{ style: {{}} }},
  bulkLocationPanel: {{ style: {{}} }},
  tabSingle: {{ style: {{}} }},
  tabBulk: {{ style: {{}} }},
  locationModalFooter: {{ style: {{}} }},
  bulkLocationRows: {{ innerHTML: "" }},
  bulkLocationCommandOutput: {{ textContent: "" }},
  locationInput: {{ value: "Leh, Ladakh" }},
  locationScopeSelect: {{ value: "trip" }},
  locationSubdirRow: {{ style: {{}} }},
  locationSubdirSelect: {{ innerHTML: "", value: "" }},
}};

global.document = {{
  getElementById: (id) => domElements[id] || {{ textContent: "", style: {{}}, classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  addEventListener: () => {{}},
}};

global.window = {{
  IntersectionObserver: undefined,
}};

// Inject catalog script
{js_code}

// Test directory discovery and decomposition
const subdirs = getAllSubdirectories();
if (!subdirs.includes("Leh Valley") || !subdirs.includes("Leh Valley/Shey Marshes")) {{
  console.error("Subdirectories decomposition missing expected values:", subdirs);
  process.exit(1);
}}

// Test bulk rows manipulation
addBulkLocationRow();
updateBulkRow(0, "subdir", "Leh Valley");
updateBulkRow(0, "location", "Leh Palace");

addBulkLocationRow();
updateBulkRow(1, "subdir", "Leh Valley/Shey Marshes");
updateBulkRow(1, "location", 'Shey "Marshes"');

addBulkLocationRow();
updateBulkRow(2, "subdir", "Chambal");
updateBulkRow(2, "location", "   "); // Empty location, should be ignored

renderBulkLocationPreview();

const previewText = domElements.bulkLocationCommandOutput.textContent;
if (!previewText.includes('avicull location --dir "Leh Valley" --location "Leh Palace"')) {{
  console.error("Preview text missing valid row 0 command. Got:", previewText);
  process.exit(1);
}}
if (!previewText.includes('avicull location --dir "Leh Valley/Shey Marshes" --location "Shey \\"Marshes\\""')) {{
  console.error("Preview text missing escaped row 1 command. Got:", previewText);
  process.exit(1);
}}
if (previewText.includes('Chambal')) {{
  console.error("Preview text should not contain empty location row Chambal. Got:", previewText);
  process.exit(1);
}}

// Test applying bulk location to batch
applyBulkLocationBatch();

if (batchCommands.length !== 1) {{
  console.error("Expected batchCommands to have 1 entry, got:", batchCommands.length);
  process.exit(1);
}}

if (batchCommands[0].stem !== "bulk_location_overrides") {{
  console.error("Expected stem 'bulk_location_overrides', got:", batchCommands[0].stem);
  process.exit(1);
}}

if (String(domElements.batchCount.textContent) !== "1") {{
  console.error("Expected batchCount '1', got:", domElements.batchCount.textContent);
  process.exit(1);
}}

console.log("NODE BULK LOCATION TEST PASSED");
"#
    );

    let test_js_file = dir.path().join("test_bulk_runner.js");
    std::fs::write(&test_js_file, runner_js).expect("write test_bulk_runner.js");

    let node_status = std::process::Command::new("node")
        .arg(&test_js_file)
        .status()
        .expect("failed to execute node");

    assert!(
        node_status.success(),
        "Node bulk location test runner must succeed"
    );
}
