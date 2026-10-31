use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{OrganizationMode, RoutingTier, SpeciesPrediction};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_item(id: &str, stem: &str, subdir: &str, species: &str, confidence: f32) -> CatalogItem {
    CatalogItem {
        bundle_id: id.into(),
        original_stem: stem.into(),
        current_path: PathBuf::from(format!("{}/{}.jpg", subdir, stem)),
        primary_image: PathBuf::from(format!("{}/{}.jpg", subdir, stem)),
        companion_images: vec![],
        sidecar_files: vec![],
        relative_preview_path: None,
        is_local: Some(false),
        assigned_species: species.into(),
        scientific_name: None,
        routing_tier: if confidence >= 0.85 {
            RoutingTier::ConfirmedSpecies
        } else {
            RoutingTier::NeedsReview
        },
        confidence,
        capture_date: Some("2026-02-07".into()),
        location_name: Some("Test Loc".into()),
        location_source: None,
        subdirectory: Some(subdir.to_string()),
        top_predictions: vec![
            SpeciesPrediction {
                taxon_id: "tx1".into(),
                common_name: species.into(),
                folder_slug: species.replace(' ', "_"),
                scientific_name: "Test sci".into(),
                visual_prob: confidence,
                geo_prior: 1.0,
                combined_score: confidence,
            },
            SpeciesPrediction {
                taxon_id: "tx2".into(),
                common_name: "Alternative Bird".into(),
                folder_slug: "Alternative_Bird".into(),
                scientific_name: "Alt sci".into(),
                visual_prob: 0.25,
                geo_prior: 1.0,
                combined_score: 0.25,
            },
        ],
    }
}

#[test]
fn test_reclassify_modal_css_prevents_flex_collapse() {
    let dir = tempdir().unwrap();
    let items = vec![make_item(
        "1",
        "IMG_001",
        "nagarhole",
        "Rufous Treepie",
        0.95,
    )];
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test",
        &items,
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();

    // The diff table container must prevent flex shrink and have a min-height
    assert!(
        html.contains(".diff-table-container {") || html.contains(".diff-table-container{"),
        "diff-table-container class missing"
    );
    assert!(
        html.contains("flex-shrink: 0") || html.contains("flex-shrink:0"),
        "diff-table-container must have flex-shrink: 0 to prevent collapsing in flex modal body"
    );
    assert!(
        html.contains("min-height:") || html.contains("min-height :"),
        "diff-table-container must have min-height"
    );

    // Command box must have overflow-y auto and bounded max-height
    assert!(
        html.contains("#reclassifyCommandOutput") || html.contains(".command-box"),
        "command box selector missing"
    );
    assert!(
        html.contains("max-height: 180px")
            || html.contains("max-height: 200px")
            || html.contains("max-height: 160px"),
        "reclassify command box must have a max-height limit"
    );

    // CX Update checks:
    // 1. Trigger button label must be "Review your classifications"
    assert!(
        html.contains("Review your classifications"),
        "Expected button label 'Review your classifications' in HTML"
    );
    // 2. Reclassify modal title must be "Review Your Classifications"
    assert!(
        html.contains("<h2>Review Your Classifications</h2>"),
        "Expected modal title 'Review Your Classifications' in HTML"
    );
    // 3. Checkboxes for 'Reclassify on location change' must exist in Single and Bulk tabs
    assert!(
        html.contains("id=\"reclassifyOnLocationChange\""),
        "Single tab reclassifyOnLocationChange toggle missing"
    );
    assert!(
        html.contains("id=\"bulkReclassifyOnLocationChange\""),
        "Bulk tab bulkReclassifyOnLocationChange toggle missing"
    );
}

#[test]
fn test_location_command_reclassify_toggle_in_node() {
    let dir = tempdir().unwrap();
    let items = vec![make_item("gir_1", "GIR_001", "Gir", "Indian Peafowl", 0.9)];
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
let singleReclassifyChecked = true;
let bulkReclassifyChecked = true;

const domElements = {{
  "catalog-data": {{ textContent: String.raw`{embedded_json}` }},
  batchCount: {{ textContent: "0" }},
  batchCountCopy: {{ textContent: "0" }},
  locationModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  reclassifyModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  locationScopeSelect: {{ value: "directory" }},
  locationSubdirSelect: {{ value: "Gir" }},
  locationInput: {{ value: "21.124, 70.824" }},
  tabSingle: {{ style: {{}} }},
  tabBulk: {{ style: {{}} }},
  reclassifyOnLocationChange: {{
    get checked() {{ return singleReclassifyChecked; }},
    set checked(val) {{ singleReclassifyChecked = val; }}
  }},
  bulkReclassifyOnLocationChange: {{
    get checked() {{ return bulkReclassifyChecked; }},
    set checked(val) {{ bulkReclassifyChecked = val; }}
  }},
  reclassifyCommandOutput: {{ textContent: "" }},
  locationCommandOutput: {{ textContent: "" }},
  bulkLocationCommandOutput: {{ textContent: "" }},
}};

global.document = {{
  getElementById: (id) => domElements[id] || {{
    textContent: "",
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: () => {{}},
    value: "",
    checked: true,
  }},
  createElement: (tag) => ({{
    tag,
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: () => {{}},
  }}),
  addEventListener: () => {{}},
}};

global.window = {{}};

{js_code}

// Case 1: singleReclassifyChecked is true -> commands must include --reclassify
singleReclassifyChecked = true;
const cmdsWithReclass = buildLocationCommands("21.124, 70.824").join("\n");
if (!cmdsWithReclass.includes('--reclassify')) {{
  console.error("Expected --reclassify flag in single location commands when checked, got:", cmdsWithReclass);
  process.exit(1);
}}

// Case 2: singleReclassifyChecked is false -> commands must NOT include --reclassify
singleReclassifyChecked = false;
const cmdsWithoutReclass = buildLocationCommands("21.124, 70.824").join("\n");
if (cmdsWithoutReclass.includes('--reclassify')) {{
  console.error("Expected NO --reclassify flag when unchecked, got:", cmdsWithoutReclass);
  process.exit(1);
}}

// Case 3: Bulk location commands with bulkReclassifyChecked true vs false
bulkLocationRowsData = [{{ subdir: "Gir", location: "21.124, 70.824" }}];

bulkReclassifyChecked = true;
const bulkCmdsWith = buildBulkLocationCommands().join("\n");
if (!bulkCmdsWith.includes('--reclassify')) {{
  console.error("Expected --reclassify flag in bulk location commands when checked, got:", bulkCmdsWith);
  process.exit(1);
}}

bulkReclassifyChecked = false;
const bulkCmdsWithout = buildBulkLocationCommands().join("\n");
if (bulkCmdsWithout.includes('--reclassify')) {{
  console.error("Expected NO --reclassify flag in bulk commands when unchecked, got:", bulkCmdsWithout);
  process.exit(1);
}}

console.log("RECLASSIFY TOGGLE NODE TEST PASSED");
"#
    );

    let test_js_file = dir.path().join("test_reclassify_toggle_runner.js");
    std::fs::write(&test_js_file, runner_js).expect("write test runner");

    let node_status = std::process::Command::new("node")
        .arg(&test_js_file)
        .status()
        .expect("failed to execute node");

    assert!(
        node_status.success(),
        "Node reclassify toggle test runner must succeed"
    );
}

#[test]
fn test_reclassify_modal_directory_scoping_and_commands_in_node() {
    let dir = tempdir().unwrap();
    let mut items = Vec::new();
    // 5 items in gir
    for i in 0..5 {
        items.push(make_item(
            &format!("gir_{}", i),
            &format!("GIR_{}", i),
            "Gir",
            "Indian Peafowl",
            0.9,
        ));
    }
    // 3 items in nagarhole
    for i in 0..3 {
        items.push(make_item(
            &format!("nh_{}", i),
            &format!("NH_{}", i),
            "nagarhole",
            "Rufous Treepie",
            0.6,
        ));
    }

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
let appendedRows = [];
const domElements = {{
  "catalog-data": {{ textContent: String.raw`{embedded_json}` }},
  batchCount: {{ textContent: "0" }},
  batchCountCopy: {{ textContent: "0" }},
  locationModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  reclassifyModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  locationScopeSelect: {{ value: "directory" }},
  locationSubdirSelect: {{ value: "nagarhole" }},
  locationInput: {{ value: "12.06000, 76.15111" }},
  tabSingle: {{ style: {{}} }},
  tabBulk: {{ style: {{}} }},
  reclassifyChangeCount: {{ textContent: "0" }},
  reclassifyDiffTableBody: {{
    textContent: "",
    appendChild: (tr) => appendedRows.push(tr),
  }},
  reclassifyCommandOutput: {{ textContent: "" }},
}};

global.document = {{
  getElementById: (id) => domElements[id] || {{
    textContent: "",
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: () => {{}},
  }},
  createElement: (tag) => ({{
    tag,
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: () => {{}},
  }}),
  addEventListener: () => {{}},
}};

global.window = {{}};

// Inject catalog script
{js_code}

// Test openReclassifyReviewModal with subdirectory scope
openReclassifyReviewModal();

// Verify reclassifyDiffItems only scoped to nagarhole (3 items, not 8)
if (reclassifyDiffItems.length !== 3) {{
  console.error("Expected 3 items scoped to nagarhole, got:", reclassifyDiffItems.length);
  process.exit(1);
}}

// Verify generated command contains location reclassify
const cmds = buildReclassifyCommands().join("\n");
if (!cmds.includes('avicull location --dir "nagarhole" --location "12.06000, 76.15111" --reclassify')) {{
  console.error("Expected location --reclassify command, got:", cmds);
  process.exit(1);
}}

console.log("RECLASSIFY MODAL TEST PASSED");
"#
    );

    let test_js_file = dir.path().join("test_reclassify_runner.js");
    std::fs::write(&test_js_file, runner_js).expect("write test runner");

    let node_status = std::process::Command::new("node")
        .arg(&test_js_file)
        .status()
        .expect("failed to execute node");

    assert!(
        node_status.success(),
        "Node reclassify modal test runner must succeed"
    );
}

#[test]
fn test_reclassify_modal_empty_state_in_node() {
    let dir = tempdir().unwrap();
    let items = vec![make_item("1", "IMG_001", "Gir", "Indian Peafowl", 0.9)];

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
let appendedRows = [];
const domElements = {{
  "catalog-data": {{ textContent: String.raw`{embedded_json}` }},
  batchCount: {{ textContent: "0" }},
  batchCountCopy: {{ textContent: "0" }},
  locationModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  reclassifyModal: {{ classList: {{ add: () => {{}}, remove: () => {{}} }} }},
  locationScopeSelect: {{ value: "directory" }},
  locationSubdirSelect: {{ value: "non_existent_folder" }},
  locationInput: {{ value: "12.06000, 76.15111" }},
  tabSingle: {{ style: {{}} }},
  tabBulk: {{ style: {{}} }},
  reclassifyChangeCount: {{ textContent: "0" }},
  reclassifyDiffTableBody: {{
    textContent: "",
    appendChild: (tr) => appendedRows.push(tr),
  }},
  reclassifyCommandOutput: {{ textContent: "" }},
}};

global.document = {{
  getElementById: (id) => domElements[id] || {{
    textContent: "",
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: () => {{}},
  }},
  createElement: (tag) => ({{
    tag,
    style: {{}},
    classList: {{ add: () => {{}}, remove: () => {{}} }},
    appendChild: (child) => appendedRows.push(child),
  }}),
  addEventListener: () => {{}},
}};

global.window = {{}};

// Inject catalog script
{js_code}

openReclassifyReviewModal();

if (reclassifyDiffItems.length !== 0) {{
  console.error("Expected 0 items for non-existent subdir, got:", reclassifyDiffItems.length);
  process.exit(1);
}}

// Should have appended an empty state row
if (appendedRows.length === 0) {{
  console.error("Expected an empty state row to be appended");
  process.exit(1);
}}

console.log("EMPTY STATE TEST PASSED");
"#
    );

    let test_js_file = dir.path().join("test_reclassify_empty_runner.js");
    std::fs::write(&test_js_file, runner_js).expect("write test runner");

    let node_status = std::process::Command::new("node")
        .arg(&test_js_file)
        .status()
        .expect("failed to execute node");

    assert!(
        node_status.success(),
        "Node reclassify empty state test runner must succeed"
    );
}
