## Task 2: Bulk Location Update by Subdirectory in the Location Modal

**Problem:** Users cannot currently set different location overrides for multiple subdirectories in one session. The location modal supports only one `--dir <subdir>` at a time. We need a "Directory Overrides" tab that shows a table of (subdirectory, location) rows that the user can add/remove/edit, and then generates a block of shell commands for all rows at once.

**Design decisions:**
- The new panel is a second tab inside the **existing** `locationModal`, alongside the existing single-update form. No new modal needed.
- Subdirectory list for autocomplete/dropdown is already in `CATALOG_DATA.directories` (populated by `CatalogPayload.directories`).
- Each row: a `<select>` for subdirectory (all known subdirectories including ancestors) + a `<input type="text">` for location + a remove button.
- "Add Row" button appends a new empty row.
- Preview command box shows all commands for all non-empty rows.
- "Add to Batch Script" / "Copy Commands" buttons apply to all rows.
- Rows with an empty location field are silently excluded from generated commands.

**Files:**
- Modify: `src/catalog/template.rs` (location modal HTML section ~L962–1065, JS section ~L2010–2113)
- Test: `tests/test_bulk_location_modal.rs` (new file) — tests the *generated HTML structure*, not runtime JS behavior (JS testing via existing playwright e2e suite if present)

**Interfaces:**
- Consumes: `CATALOG_DATA.directories: string[]` (already populated); existing `buildLocationCommands`, `openLocationModal`, `closeLocationModal`, `populateSubdirectories` JS functions.
- Produces: New JS functions `addBulkLocationRow()`, `removeBulkLocationRow(idx)`, `renderBulkLocationPreview()`, `applyBulkLocationBatch()`; new HTML tab structure inside `#locationModal`.

- [ ] **Step 1: Write failing test for bulk location modal structure**

  Create `tests/test_bulk_location_modal.rs`:

  ```rust
  use avicull::catalog::{generate_html_catalog, CatalogItem};
  use avicull::types::{OrganizationMode, RoutingTier};
  use tempfile::tempdir;
  use std::path::PathBuf;

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
      generate_html_catalog("Test", &items, OrganizationMode::InPlaceFileRenameToDescriptiveNames, &out).unwrap();
      let html = std::fs::read_to_string(&out).unwrap();
      assert!(html.contains("bulkLocationRows") || html.contains("bulk-location"), 
              "bulk location rows container not found");
      assert!(html.contains("addBulkLocationRow"), "addBulkLocationRow function not found");
      assert!(html.contains("renderBulkLocationPreview"), "renderBulkLocationPreview function not found");
  }

  #[test]
  fn test_bulk_location_commands_exclude_empty_location_rows() {
      // This verifies the JS function body in the template contains the empty-location guard
      let dir = tempdir().unwrap();
      let items = vec![item_with_subdir("Leh Valley")];
      let out = dir.path().join("bird_catalog.html");
      generate_html_catalog("Test", &items, OrganizationMode::InPlaceFileRenameToDescriptiveNames, &out).unwrap();
      let html = std::fs::read_to_string(&out).unwrap();
      // The filter for non-empty location in renderBulkLocationPreview must be present
      assert!(html.contains("row.location") || html.contains(".location.trim()"),
              "empty location guard not found in bulk preview function");
  }
  ```

- [ ] **Step 2: Run tests to verify they fail**

  ```bash
  cargo test test_bulk_location_tab_present_in_modal test_bulk_location_commands_exclude_empty_location_rows 2>&1 | tail -20
  ```
  Expected: FAIL.

- [ ] **Step 3: Add tab navigation structure to location modal HTML in `template.rs`**

  Replace the existing `#locationModal` modal-body content (around lines 968–1059) with a tabbed layout:

  **Tab 1 (default active):** "Single / Selection" — the existing scope select + input + reclassify button (unchanged).

  **Tab 2:** "Directory Overrides" — a new panel containing:
  ```html
  <div id="bulkLocationPanel" style="display:none;">
    <p style="font-size:0.82rem; color:var(--text-secondary); margin-bottom:8px;">
      Set a different location for each subdirectory. Each row generates an independent
      <code>avicull location --dir … --location …</code> command.
    </p>
    <div id="bulkLocationRows"></div>
    <button class="btn btn-secondary" style="margin-top:8px;" onclick="addBulkLocationRow()">+ Add Directory Row</button>
    <div style="margin-top:14px;">
      <label style="font-size:0.85rem;font-weight:600;color:var(--text-secondary);margin-bottom:6px;display:block;">
        Generated Commands:
      </label>
      <div class="command-box" id="bulkLocationCommandOutput">Add rows above to generate commands…</div>
    </div>
  </div>
  ```

  Add two tab buttons above the modal-body:
  ```html
  <div style="display:flex;gap:8px;margin-bottom:16px;border-bottom:1px solid var(--border-color);padding-bottom:8px;">
    <button id="tabSingle" class="btn btn-secondary" onclick="switchLocationTab('single')" 
            style="font-size:0.8rem;padding:4px 12px;">Single / Selection</button>
    <button id="tabBulk" class="btn btn-secondary" onclick="switchLocationTab('bulk')" 
            style="font-size:0.8rem;padding:4px 12px;">Directory Overrides</button>
  </div>
  ```

- [ ] **Step 4: Add JS functions in `template.rs`**

  Add these new JS functions after `closeLocationModal()` (around line 2082):

  ```js
  // --- Bulk Location Override by Subdirectory ---

  // State: array of {subdir: string, location: string}
  let bulkLocationRowsData = [];

  function switchLocationTab(tab) {{
    const singlePanel = document.getElementById('locationSinglePanel');
    const bulkPanel = document.getElementById('bulkLocationPanel');
    const tabSingle = document.getElementById('tabSingle');
    const tabBulk = document.getElementById('tabBulk');
    if (tab === 'bulk') {{
      if (singlePanel) singlePanel.style.display = 'none';
      if (bulkPanel) bulkPanel.style.display = 'block';
      if (tabBulk) tabBulk.style.background = 'var(--accent-blue)';
      if (tabSingle) tabSingle.style.background = '';
      if (bulkLocationRowsData.length === 0) addBulkLocationRow();
    }} else {{
      if (singlePanel) singlePanel.style.display = 'block';
      if (bulkPanel) bulkPanel.style.display = 'none';
      if (tabSingle) tabSingle.style.background = 'var(--accent-blue)';
      if (tabBulk) tabBulk.style.background = '';
    }}
  }}

  function addBulkLocationRow() {{
    bulkLocationRowsData.push({{ subdir: '', location: '' }});
    renderBulkLocationRows();
  }}

  function removeBulkLocationRow(idx) {{
    bulkLocationRowsData.splice(idx, 1);
    renderBulkLocationRows();
  }}

  function updateBulkRow(idx, field, value) {{
    if (bulkLocationRowsData[idx]) {{
      bulkLocationRowsData[idx][field] = value;
      renderBulkLocationPreview();
    }}
  }}

  function renderBulkLocationRows() {{
    const container = document.getElementById('bulkLocationRows');
    if (!container) return;

    // Build sorted list of all known subdirs (ancestors included) from CATALOG_DATA.directories + items
    const dirSet = new Set(CATALOG_DATA.directories || []);
    items.forEach(item => {{ if (item.subdirectory) dirSet.add(item.subdirectory); }});
    const sortedDirs = Array.from(dirSet).sort();

    container.innerHTML = bulkLocationRowsData.map((row, idx) => {{
      const opts = sortedDirs.map(d => {{
        const depth = (d.match(/\//g) || []).length;
        const indent = '\u00A0\u00A0'.repeat(depth);
        const prefix = depth > 0 ? '\u2514\u2500 ' : '\uD83D\uDCC1 ';
        const sel = row.subdir === d ? ' selected' : '';
        return `<option value="${{d}}"${{sel}}>${{indent}}${{prefix}}${{d}}</option>`;
      }}).join('');

      return `
        <div class="bulk-loc-row" style="display:flex;gap:8px;align-items:center;margin-bottom:8px;">
          <select style="flex:0 0 200px;" onchange="updateBulkRow(${{idx}},'subdir',this.value)">
            <option value="">-- Subdirectory --</option>
            ${{opts}}
          </select>
          <input type="text" style="flex:1;" placeholder="Location (place name or lat,lon)"
                 value="${{row.location}}" oninput="updateBulkRow(${{idx}},'location',this.value)"/>
          <button class="btn btn-secondary" style="padding:4px 8px;font-size:0.8rem;"
                  onclick="removeBulkLocationRow(${{idx}})">✕</button>
        </div>
      `;
    }}).join('');

    renderBulkLocationPreview();
  }}

  function renderBulkLocationPreview() {{
    const box = document.getElementById('bulkLocationCommandOutput');
    if (!box) return;

    const validRows = bulkLocationRowsData.filter(r => r.subdir && r.location.trim());
    if (validRows.length === 0) {{
      box.textContent = 'Add rows above to generate commands…';
      return;
    }}

    let lines = [];
    if (CATALOG_DATA.catalog_dir) lines.push(`cd "${{CATALOG_DATA.catalog_dir}}"`);
    lines.push(findAvicullCmd().trim());
    lines.push(`if [ -n "$AVICULL_CMD" ]; then`);
    validRows.forEach(r => {{
      lines.push(`  avicull location --dir "${{r.subdir}}" --location "${{r.location}}" 2>/dev/null || $AVICULL_CMD location --dir "${{r.subdir}}" --location "${{r.location}}"`);
    }});
    lines.push(`fi`);
    box.textContent = lines.join('\n');
  }}

  function copyBulkLocationCommands() {{
    const box = document.getElementById('bulkLocationCommandOutput');
    if (!box) return;
    navigator.clipboard.writeText(box.textContent).catch(() => {{
      const ta = document.createElement('textarea');
      ta.value = box.textContent;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
    }});
  }}

  function applyBulkLocationBatch() {{
    const box = document.getElementById('bulkLocationCommandOutput');
    if (!box || !box.textContent.trim() || box.textContent.startsWith('Add rows')) return;
    batchCommands.push(box.textContent);
    updateBatchUI();
    closeLocationModal();
  }}
  ```

  Also add "Copy Commands" and "Add to Batch Script" buttons in the bulk panel footer (inside `.modal-footer` or as inline buttons below `#bulkLocationCommandOutput`):
  ```html
  <div style="display:flex;gap:8px;margin-top:8px;">
    <button class="btn btn-secondary" onclick="copyBulkLocationCommands()">Copy Commands</button>
    <button class="btn btn-primary" onclick="applyBulkLocationBatch()">Add to Batch Script</button>
  </div>
  ```

- [ ] **Step 5: Run tests to verify they pass**

  ```bash
  cargo test test_bulk_location_tab_present_in_modal test_bulk_location_commands_exclude_empty_location_rows 2>&1 | tail -20
  ```
  Expected: 2 PASS.

- [ ] **Step 6: Run full test suite and lint**

  ```bash
  cargo test 2>&1 | tail -20
  cargo fmt --check
  cargo clippy -- -D warnings
  ```
  Expected: all green.

- [ ] **Step 7: Manual smoke-test** (open fixture HTML in a browser)

  ```bash
  # Regenerate test fixture via a dry-run or direct catalog generation test
  cargo test 2>&1 | tail -5
  # Open the generated HTML in a browser (macOS)
  # open /tmp/bird_catalog_test.html  — or use the fixture path printed by a test
  ```
  Verify:
  1. Location modal opens → two tabs visible.
  2. "Directory Overrides" tab → click → add row → subdir dropdown populated with `Leh Valley`, `Leh Valley/Shey Marshes`, `Chambal Sanctuary`.
  3. Fill in location for each → commands preview updates.
  4. Row with blank location → excluded from commands.
  5. "Copy Commands" → clipboard has valid shell block.

- [ ] **Step 8: Commit**

  ```bash
  git add src/catalog/template.rs tests/test_bulk_location_modal.rs
  git commit -m "feat(catalog): add Directory Overrides tab for bulk subdirectory location updates"
  ```

---

## Verification Agent Crew Dispatch (post-implementation)

After both tasks are implemented and green, dispatch the three-agent crew required by AGENTS.md:

1. **correctness-safety-reviewer** — verify source immutability, no hardcoded paths, no unsafe HTML injection from item data.
2. **rust-readability-reviewer** — verify idiomatic Rust, no procedural meta-comments in template string.
3. **spec-conformance-reviewer** — verify card title = species only, date on own line, bulk commands correct format.

Remediate any findings before marking complete.
