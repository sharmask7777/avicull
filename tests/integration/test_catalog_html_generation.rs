// Verifies all aspects of HTML catalog generation:
// 1. Card Layout & Metadata Display (species, separate date and location lines)
// 2. Directory Overrides Tab & Bulk Location Management
// 3. Reclassification Review Modal DOM & Script Execution
use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{OrganizationMode, RoutingTier, SpeciesPrediction};
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn make_card_item(species: &str, date: Option<&str>, location: Option<&str>) -> CatalogItem {
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

fn make_reclassify_item(
    id: &str,
    stem: &str,
    subdir: &str,
    species: &str,
    confidence: f32,
) -> CatalogItem {
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

// ---------------------------------------------------------------------------
// 1. Card Rendering Tests
// ---------------------------------------------------------------------------

#[test]
fn test_catalog_html_card_title_displays_species_only() {
    let dir = tempdir().unwrap();
    let item = make_card_item("Oriental Magpie-Robin", Some("2025-05-18"), None);
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(
        html.contains("Oriental Magpie-Robin"),
        "species name absent from HTML"
    );
    assert!(
        !html.contains("20250518"),
        "raw date slug must not appear in HTML"
    );
    assert!(
        !html.contains("K_Gudi"),
        "location slug must not appear in HTML"
    );
}

#[test]
fn test_catalog_html_card_renders_date_and_location_on_distinct_lines() {
    let dir = tempdir().unwrap();
    let item = make_card_item(
        "Rose-ringed Parakeet",
        Some("2025-05-18"),
        Some("K Gudi Wilderness"),
    );
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();

    assert!(
        html.contains("class=\"card-date\""),
        "expected card-date element in HTML"
    );
    assert!(
        html.contains("class=\"card-location\""),
        "expected card-location element in HTML"
    );
}

#[test]
fn test_catalog_html_card_omits_date_line_when_capture_date_is_null() {
    let dir = tempdir().unwrap();
    let item = make_card_item("Oriental Magpie-Robin", None, Some("K Gudi"));
    let out = dir.path().join("bird_catalog.html");
    generate_html_catalog(
        "Test Trip",
        &[item],
        OrganizationMode::InPlaceFileRenameToDescriptiveNames,
        &out,
    )
    .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();

    assert!(
        html.contains("\"capture_date\":null"),
        "capture_date must be null in payload when no date provided"
    );
    assert!(
        !html.contains("2025-"),
        "no date string should appear in the HTML when no date is set"
    );
}

// ---------------------------------------------------------------------------
// 2. Directory Overrides Tab & Bulk Location Management
// ---------------------------------------------------------------------------

#[test]
fn test_catalog_html_location_modal_contains_bulk_directory_elements() {
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
        html.contains("renderBulkLocationRows"),
        "renderBulkLocationRows JS function not found"
    );
    assert!(
        html.contains("renderBulkLocationPreview"),
        "renderBulkLocationPreview JS function not found"
    );
    assert!(
        html.contains("buildBulkLocationCommands"),
        "buildBulkLocationCommands JS function not found"
    );
}

#[test]
fn test_catalog_html_location_modal_excludes_empty_location_rows() {
    let dir = tempdir().unwrap();
    let items = vec![item_with_subdir("Trip A"), item_with_subdir("Trip B")];
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
        html.contains("!r.location || !r.location.trim()")
            || html.contains("r.location && r.location.trim()"),
        "Empty location guard not found in renderBulkLocationPreview / buildBulkLocationCommands"
    );
}

#[test]
fn test_catalog_html_node_script_executes_bulk_location_batch() {
    let dir = tempdir().unwrap();
    let items = vec![
        item_with_subdir("Trip A"),
        item_with_subdir("Trip A/Morning"),
        item_with_subdir("Trip B"),
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

    let script_start = html
        .find("<script>\n  let CATALOG_DATA")
        .expect("script start");
    let script_end = html.rfind("</script>").expect("script end");
    let script_content = &html[script_start + 8..script_end];

    let runner_js = format!(
        r#"
const jsdom = (() => {{
  let domElements = {{}};
  return {{
    getElementById: (id) => {{
      if (!domElements[id]) {{
        domElements[id] = {{
          value: '',
          textContent: '',
          innerHTML: '',
          style: {{}},
          classList: {{
            add: () => {{}},
            remove: () => {{}},
            contains: () => false
          }}
        }};
      }}
      return domElements[id];
    }},
    querySelectorAll: () => [],
    addEventListener: () => {{}}
  }};
}})();

global.document = jsdom;
global.window = {{
  addEventListener: () => {{}},
  location: {{ search: '' }}
}};
global.navigator = {{ clipboard: {{ writeText: () => Promise.resolve() }} }};

{script_content}

bulkLocationRowsData = [
  {{ subdir: 'Trip A', location: 'Leh Ladakh' }},
  {{ subdir: 'Trip A/Morning', location: '' }},
  {{ subdir: 'Trip B', location: '   ' }}
];

const cmds = buildBulkLocationCommands();
const cmdsText = cmds.join('\n');

if (!cmdsText.includes('--location "Leh Ladakh"')) {{
  console.error("FAIL: Expected location 'Leh Ladakh' in commands, got:\n" + cmdsText);
  process.exit(1);
}}
if (cmdsText.includes('Trip B') || cmdsText.includes('Trip A/Morning')) {{
  console.error("FAIL: Empty location rows should not produce commands, got:\n" + cmdsText);
  process.exit(1);
}}

console.log("NODE BULK LOCATION TEST PASSED");
"#,
        script_content = script_content
    );

    let runner_path = dir.path().join("test_runner.js");
    std::fs::write(&runner_path, runner_js).unwrap();

    let output = Command::new("node")
        .arg(&runner_path)
        .output()
        .expect("Failed to execute node. Ensure node is installed.");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Node script failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
        output.status.code(),
        stdout,
        stderr
    );
    assert!(stdout.contains("NODE BULK LOCATION TEST PASSED"));
}

// ---------------------------------------------------------------------------
// 3. Reclassification Review Modal DOM & Script Execution
// ---------------------------------------------------------------------------

#[test]
fn test_catalog_html_reclassify_modal_prevents_css_flex_collapse() {
    let dir = tempdir().unwrap();
    let items = vec![make_reclassify_item(
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

    assert!(
        html.contains("Review your classifications"),
        "Expected button label 'Review your classifications' in HTML"
    );
    assert!(
        html.contains("<h2>Review Your Classifications</h2>"),
        "Expected modal title 'Review Your Classifications' in HTML"
    );
    assert!(
        html.contains("id=\"reclassifyOnLocationChange\""),
        "Single tab reclassifyOnLocationChange toggle missing"
    );
    assert!(
        html.contains("id=\"bulkReclassifyOnLocationChange\""),
        "Bulk tab bulkReclassifyOnLocationChange toggle missing"
    );
    assert!(
        html.contains("id=\"btnMainReviewClassifications\""),
        "Dedicated 'Review your classifications' button missing from header in main page"
    );
    assert!(
        html.contains("Location of the current directory:"),
        "Expected explicit label 'Location of the current directory:' in header"
    );
    assert!(
        html.contains("id=\"btnModifyLocations\""),
        "Expected dedicated button 'btnModifyLocations' in header"
    );
}

#[test]
fn test_catalog_html_node_script_location_reclassify_toggle_updates_commands() {
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

    let script_start = html
        .find("<script>\n  let CATALOG_DATA")
        .expect("script start");
    let script_end = html.rfind("</script>").expect("script end");
    let script_content = &html[script_start + 8..script_end];

    let runner_js = format!(
        r#"
const jsdom = (() => {{
  let domElements = {{}};
  return {{
    getElementById: (id) => {{
      if (!domElements[id]) {{
        domElements[id] = {{
          value: '',
          checked: false,
          textContent: '',
          innerHTML: '',
          style: {{}},
          classList: {{
            add: () => {{}},
            remove: () => {{}},
            contains: () => false
          }}
        }};
      }}
      return domElements[id];
    }},
    querySelectorAll: () => [],
    addEventListener: () => {{}}
  }};
}})();

global.document = jsdom;
global.window = {{
  addEventListener: () => {{}},
  location: {{ search: '' }}
}};
global.navigator = {{ clipboard: {{ writeText: () => Promise.resolve() }} }};

{script_content}

// 1. When checked, buildLocationCommands includes --reclassify
document.getElementById('reclassifyOnLocationChange').checked = true;
let cmdsWith = buildLocationCommands("Pangong Tso");
let txtWith = cmdsWith.join('\n');
if (!txtWith.includes('--reclassify')) {{
  console.error("FAIL: Expected --reclassify when checked, got:\n" + txtWith);
  process.exit(1);
}}

// 2. When unchecked, buildLocationCommands omits --reclassify
document.getElementById('reclassifyOnLocationChange').checked = false;
let cmdsWithout = buildLocationCommands("Pangong Tso");
let txtWithout = cmdsWithout.join('\n');
if (txtWithout.includes('--reclassify')) {{
  console.error("FAIL: Expected no --reclassify when unchecked, got:\n" + txtWithout);
  process.exit(1);
}}

// 3. Bulk location commands also respect bulkReclassifyOnLocationChange
bulkLocationRowsData = [
  {{ subdir: 'Leh Valley', location: 'Hemis' }}
];
document.getElementById('bulkReclassifyOnLocationChange').checked = true;
let bulkWith = buildBulkLocationCommands().join('\n');
if (!bulkWith.includes('--reclassify')) {{
  console.error("FAIL: Expected --reclassify in bulk when checked, got:\n" + bulkWith);
  process.exit(1);
}}

document.getElementById('bulkReclassifyOnLocationChange').checked = false;
let bulkWithout = buildBulkLocationCommands().join('\n');
if (bulkWithout.includes('--reclassify')) {{
  console.error("FAIL: Expected no --reclassify in bulk when unchecked, got:\n" + bulkWithout);
  process.exit(1);
}}

console.log("NODE RECLASSIFY TOGGLE TEST PASSED");
"#,
        script_content = script_content
    );

    let runner_path = dir.path().join("test_reclassify_toggle_runner.js");
    std::fs::write(&runner_path, runner_js).unwrap();

    let output = Command::new("node")
        .arg(&runner_path)
        .output()
        .expect("Failed to execute node");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Node script failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
        output.status.code(),
        stdout,
        stderr
    );
    assert!(stdout.contains("NODE RECLASSIFY TOGGLE TEST PASSED"));
}

#[test]
fn test_catalog_html_node_script_reclassify_modal_scopes_to_subdirectory() {
    let dir = tempdir().unwrap();
    let items = vec![
        make_reclassify_item("1", "IMG_001", "nagarhole", "Rufous Treepie", 0.95),
        make_reclassify_item("2", "IMG_002", "nagarhole", "Indian Peafowl", 0.90),
        make_reclassify_item("3", "IMG_003", "nagarhole", "Spotted Dove", 0.85),
        make_reclassify_item("4", "IMG_004", "kabini", "Malabar Pied Hornbill", 0.92),
        make_reclassify_item("5", "IMG_005", "kabini", "Osprey", 0.88),
        make_reclassify_item("6", "IMG_006", "wayanad", "White-bellied Treepie", 0.91),
        make_reclassify_item("7", "IMG_007", "wayanad", "Jerdon's Nightjar", 0.80),
        make_reclassify_item("8", "IMG_008", "bandipur", "Grey Junglefowl", 0.89),
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

{js_code}

openReclassifyReviewModal();

if (reclassifyDiffItems.length !== 3) {{
  console.error("Expected 3 items scoped to nagarhole, got:", reclassifyDiffItems.length);
  process.exit(1);
}}

const cmds = buildReclassifyCommands().join("\n");
if (!cmds.includes('avicull location --dir "nagarhole" --location "12.06000, 76.15111" --reclassify')) {{
  console.error("Expected location --reclassify command, got:", cmds);
  process.exit(1);
}}

console.log("RECLASSIFY MODAL TEST PASSED");
"#,
        embedded_json = embedded_json,
        js_code = js_code
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
fn test_catalog_html_node_script_reclassify_modal_renders_empty_state() {
    let dir = tempdir().unwrap();
    let items = vec![make_reclassify_item(
        "1",
        "IMG_001",
        "Gir",
        "Indian Peafowl",
        0.9,
    )];

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

{js_code}

openReclassifyReviewModal();

if (reclassifyDiffItems.length !== 0) {{
  console.error("Expected 0 items for non-existent subdir, got:", reclassifyDiffItems.length);
  process.exit(1);
}}

if (appendedRows.length === 0) {{
  console.error("Expected an empty state row to be appended");
  process.exit(1);
}}

console.log("EMPTY STATE TEST PASSED");
"#,
        embedded_json = embedded_json,
        js_code = js_code
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
