# Implementation Plan: Location Override CLI & Interactive Catalog UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide the ability to update the geographic location (GPS coordinates or named gazetteer location) per directory or globally via CLI (`avicull location`) and inside `bird_catalog.html` with real-time command generation, reclassifying and location-tagging assets accurately.

**Architecture:**
1. **CLI Subcommand `avicull location`**:
   - Accepts `--dir <PATH_OR_KEY>`, `--location <NAME_OR_COORDS>`, and optional `--db <PATH>`.
   - Resolves location against `GazetteerEngine` (or parses `lat, lon` coordinates), updates SQLite manifest table `directory_locations`, and re-tags bundles belonging to that trip directory.
   - If `--reclassify` is flagged (or run via subsequent `avicull organize --relink` / `reconcile`), recalculates geographic Bayesian priors and updates classifications based on eBird regional occurrence lists.
2. **Catalog Payload & UI Enhancements in `src/catalog/`**:
   - Add `location_name: Option<String>` and `location_coords: Option<(f64, f64)>` to `CatalogPayload` in `src/catalog/mod.rs`.
   - In `src/catalog/template.rs`, render a location badge/button in the header controls (e.g. `📍 Leh (34.15° N, 77.58° E)` or `📍 Location Unset`).
   - Add a "Set/Edit Location" modal with an input for place name (e.g. "Keoladeo National Park, Bharatpur") or coordinates ("27.159, 77.523").
   - Autocomplete or suggestions from known regions/gazetteer, generating:
     `avicull location --dir "<trip_folder>" --location "<location>"`
   - Provide an "Add to Batch Script" button so the location update runs cleanly alongside or prior to photo corrections in `apply_corrections.sh`.
3. **Automated Testing**:
   - Integration tests in `tests/test_cli_location.rs`: tests CLI location overrides, coordinates, database updates, and bundle location synchronization.
   - Playwright E2E tests in `tests/e2e/bird_catalog.spec.ts`: tests the "Edit Location" modal, inputting a location, and verifying the generated command in the UI and batch script.

**Global Constraints**:
- Source Immutability strictly observed.
- Zero external runtime server dependencies for standalone HTML.
- TDD mandatory: write failing tests before production code.

---

### Task 1: CLI Subcommand `avicull location` (TDD)
- [ ] **Step 1: Write integration test `tests/test_cli_location.rs`**
  - Test setting directory location by coordinates `lat, lon`
  - Test setting directory location by gazetteer name e.g. "Leh"
  - Verify SQLite `directory_locations` table and bundle `resolved_location_name` updates
- [ ] **Step 2: Run test and observe failure**
- [ ] **Step 3: Implement `LocationArgs` in `src/cli.rs` and `execute_location` in `src/main.rs`**
- [ ] **Step 4: Verify test passes**

### Task 2: Embed Location Information in `CatalogPayload`
- [ ] **Step 1: Add location fields to `CatalogPayload` in `src/catalog/mod.rs`**
- [ ] **Step 2: Write test in `tests/test_catalog.rs` verifying payload contains trip directory location**
- [ ] **Step 3: Implement population of trip location in `build_catalog_payload`**

### Task 3: Interactive Location UI in `bird_catalog.html`
- [ ] **Step 1: Add Location button & edit modal in `src/catalog/template.rs`**
  - Header badge displaying current location or `+ Set Location`
  - Modal with input for location name or GPS coordinates
  - Real-time command generator: `avicull location --location "..."`
  - "Add to Batch Script" button updating `apply_corrections.sh`
- [ ] **Step 2: Update Playwright E2E tests in `tests/e2e/bird_catalog.spec.ts`**
  - Verify location badge display
  - Verify opening Location Modal, typing location, generating command, and adding to batch script
- [ ] **Step 3: Run `npm run test:e2e` and verify all tests pass**

### Task 4: Linting, Formatting, and Code Review
- [ ] Run `cargo fmt --check`
- [ ] Run `cargo clippy --all-targets -- -D warnings`
- [ ] Commit cleanly with descriptive message
