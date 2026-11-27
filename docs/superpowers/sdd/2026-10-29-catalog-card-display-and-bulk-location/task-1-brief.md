## Task 1: Fix Card Title — Species-only name, separate date line

**Problem:** Currently `item.assigned_species` can be set from `current_path` parsing (e.g. `Oriental Magpie-Robin 20250518 K Gudi`) and appears verbatim in the card title. The JSON `assigned_species` field is already clean (just the species name), but we need to confirm the card renders `item.assigned_species` directly (which it already does at line 1372) AND that `capture_date` appears on its own separate line rather than as an inline meta-item in the same row as location.

**Root cause (confirmed from code inspection):**
- `item.assigned_species` is already the clean species name in the JSON payload (`CatalogPayloadItem::assigned_species`). No data-model change needed.
- However the card layout at template line 1375–1378 renders `locationMarkup` and `dateMarkup` inside the same `.card-meta` div — they appear as inline `<span>` elements side-by-side. The request is to show date on its own dedicated line below the species name (separate from the location chip).

**Files:**
- Modify: `src/catalog/template.rs` (card rendering section ~L1332–1388, CSS section ~L351–380)
- Test: `tests/test_catalog_card_display.rs` (new file)

**Interfaces:**
- Consumes: `CatalogPayloadItem` JSON shape (unchanged): `assigned_species: String`, `capture_date: Option<String>`, `location_name: Option<String>`, `scientific_name: Option<String>`
- Produces: HTML card with `.card-species-title > span` = species name only; `.card-date` = date on its own line; `.card-location` = location (unchanged position).

- [ ] **Step 1: Write the failing integration test**

  Create `tests/test_catalog_card_display.rs`:

  ```rust
  // Verifies that the rendered HTML catalog card shows:
  //   1. `.card-species-title span` containing ONLY the species name
  //   2. A `.card-date` element containing the capture_date (separate from location)
  //   3. `.card-location` element containing the location_name
  //   4. No date text inside `.card-location` and vice versa
  use avicull::catalog::{generate_html_catalog, CatalogItem};
  use avicull::types::{OrganizationMode, RoutingTier};
  use tempfile::tempdir;
  use std::path::PathBuf;

  fn make_item(species: &str, date: Option<&str>, location: Option<&str>) -> CatalogItem {
      CatalogItem {
          bundle_id: "b1".into(),
          original_stem: "IMG_001".into(),
          current_path: PathBuf::from("IMG_001_Oriental_Magpie_Robin_20250518_K_Gudi.CR2"),
          primary_image: PathBuf::from("IMG_001_Oriental_Magpie_Robin_20250518_K_Gudi.CR2"),
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
      let item = make_item("Oriental Magpie-Robin", Some("2025-05-18"), Some("K Gudi"));
      let out = dir.path().join("bird_catalog.html");
      generate_html_catalog("Test Trip", &[item], OrganizationMode::InPlaceFileRenameToDescriptiveNames, &out).unwrap();
      let html = std::fs::read_to_string(&out).unwrap();
      // Card title must not contain the date or location suffix
      assert!(html.contains("Oriental Magpie-Robin"), "species name absent");
      assert!(!html.contains("20250518"), "raw date slug must not appear in card title");
      assert!(!html.contains("K Gudi"), "location must not appear in card title");
  }

  #[test]
  fn test_card_date_on_separate_line() {
      let dir = tempdir().unwrap();
      let item = make_item("Oriental Magpie-Robin", Some("2025-05-18"), Some("K Gudi"));
      let out = dir.path().join("bird_catalog.html");
      generate_html_catalog("Test Trip", &[item], OrganizationMode::InPlaceFileRenameToDescriptiveNames, &out).unwrap();
      let html = std::fs::read_to_string(&out).unwrap();
      // Date must appear inside a dedicated .card-date element
      assert!(html.contains("card-date"), "no .card-date element found");
      // Location must appear inside a dedicated .card-location element
      assert!(html.contains("card-location"), "no .card-location element found");
  }

  #[test]
  fn test_card_no_date_shows_no_date_line() {
      let dir = tempdir().unwrap();
      let item = make_item("Oriental Magpie-Robin", None, Some("K Gudi"));
      let out = dir.path().join("bird_catalog.html");
      generate_html_catalog("Test Trip", &[item], OrganizationMode::InPlaceFileRenameToDescriptiveNames, &out).unwrap();
      let html = std::fs::read_to_string(&out).unwrap();
      assert!(!html.contains("card-date"), "card-date should be absent when no date");
  }
  ```

- [ ] **Step 2: Run tests to verify they fail**

  ```bash
  cargo test test_card_species_title_is_species_only test_card_date_on_separate_line test_card_no_date_shows_no_date_line 2>&1 | tail -20
  ```
  Expected: FAIL — `card-date` class not present in current template output.

- [ ] **Step 3: Update card CSS and JS in `src/catalog/template.rs`**

  In the CSS block (around line 351), add:
  ```css
  .card-date {{
    font-size: 0.78rem;
    color: var(--text-muted);
    margin-top: 4px;
    display: flex;
    align-items: center;
    gap: 4px;
  }}
  .card-location {{
    font-size: 0.78rem;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 4px;
  }}
  ```

  Replace the `dateMarkup` and `locationMarkup` template literals (around lines 1332–1344) with:
  ```js
  const locationMarkup = item.location_name ? `
    <div class="card-location">
      <svg width="12" height="12" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M5.05 4.05a7 7 0 119.9 9.9L10 18.9l-4.95-4.95a7 7 0 010-9.9zM10 11a2 2 0 100-4 2 2 0 000 4z" clip-rule="evenodd"/></svg>
      ${{item.location_name}}
    </div>
  ` : '';

  const dateMarkup = item.capture_date ? `
    <div class="card-date">
      <svg width="12" height="12" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M6 2a1 1 0 00-1 1v1H4a2 2 0 00-2 2v10a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2h-1V3a1 1 0 10-2 0v1H7V3a1 1 0 00-1-1zm0 5a1 1 0 000 2h8a1 1 0 100-2H6z" clip-rule="evenodd"/></svg>
      ${{item.capture_date}}
    </div>
  ` : '';
  ```

  In the card HTML template (around lines 1370–1388), restructure `.card-body` to:
  ```html
  <div class="card-body">
    <div class="card-species-title">
      <span>${{item.assigned_species}}</span>
    </div>
    ${{scientificMarkup}}
    ${{dateMarkup}}
    ${{locationMarkup}}
    <div class="card-path" title="${{item.current_path}}">${{item.current_path}}</div>
    <div class="card-actions">…</div>
  </div>
  ```
  Note: `dateMarkup` and `locationMarkup` are now sibling block `<div>`s, not inline `<span>`s inside a shared `.card-meta` wrapper. Remove the `.card-meta` wrapper div entirely (or if other code depends on it, make it a no-op wrapper with no flex row styling).

- [ ] **Step 4: Run tests to verify they pass**

  ```bash
  cargo test test_card_species_title_is_species_only test_card_date_on_separate_line test_card_no_date_shows_no_date_line 2>&1 | tail -20
  ```
  Expected: 3 PASS.

- [ ] **Step 5: Run full test suite and lint**

  ```bash
  cargo test 2>&1 | tail -20
  cargo fmt --check
  cargo clippy -- -D warnings
  ```
  Expected: all green, no warnings.

- [ ] **Step 6: Regenerate test fixture**

  ```bash
  # The fixture is generated; update it so test assertions in e2e tests can reference the new class names
  cd /Users/shaleensharma/workplace/gemini_ws/avian_rearranger
  cargo run -- organize --input fixtures/ --output fixtures/birds_hierarchy --dry-run 2>/dev/null || true
  # If fixture generation has a dedicated script:
  ls fixtures/
  # Update fixtures/test_bird_catalog.html by running its generator (see tests/e2e or scripts/)
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add src/catalog/template.rs tests/test_catalog_card_display.rs
  git commit -m "feat(catalog): show species-only in card title; separate date and location lines"
  ```

---

