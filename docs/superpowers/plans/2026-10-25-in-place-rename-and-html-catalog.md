# In-Place Rename Mode & Standalone HTML Catalog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add extensible organization modes to `avicull` with in-place file renaming as the new default (eliminating cloud storage hardlink duplication on Google Drive and Proton Drive), alongside an interactive, offline standalone `catalog.html` featuring trip statistics, search, and a mislabel correction modal that generates copyable Unix `mv` commands and batch correction scripts.

**Architecture:** Introduce `OrganizationMode` enum (`InPlaceFileRenameToDescriptiveNames` and `HierarchicalHardlinkHierarchy`). In in-place mode, rename primary images, companion JPEGs, and `.xmp` sidecars in-place inside `--input` to `<stem>_<Species>[_<Date>][_<Location>].<ext>` with full idempotency and SQLite manifest tracking. Generate a zero-dependency, self-contained `catalog.html` with embedded JSON metadata, eBird species autocomplete, cloud-eviction fallback tiles, and client-side `mv` command generation for bird reclassification.

**Tech Stack:** Rust (edition 2021), `clap` v4 (value_enum), `rusqlite` bundled, `serde` / `serde_json`, modern semantic HTML5 / CSS3 / vanilla JavaScript.

**Spec:** `docs/superpowers/specs/2026-10-25-in-place-rename-and-html-catalog-design.md`

## Global Constraints
- Strict source immutability when in `--mode hierarchy` (unchanged legacy behavior).
- Zero-byte payload overhead: in-place renames consume 0 additional disk space locally and in cloud storage.
- Atomic sidecar synchronization: RAW primary, `.xmp` sidecars, and companion JPEGs must always be renamed to identical base stems in lockstep.
- Offline resilience: `catalog.html` must work without internet connection (zero external CDN scripts/styles).
- No procedural agent meta-comments (`// Step 1: ...`) in code.
- Mandatory Test-Driven Development (TDD): Red $\rightarrow$ Green $\rightarrow$ Refactor.

## Review Focus
1. Sidecar or companion file left behind with old stem if primary is renamed.
2. Double-suffixing on repeated runs of `avicull organize --mode in-place`.
3. Running `avicull clean` on an in-place run accidentally deleting files instead of reverting renames.
4. Broken image preview in `catalog.html` when browsing pure RAW files lacking companion JPEGs.
5. In-place filenames with missing capture date or location inserting ugly `Unknown` tokens.

---

### Task 1: `OrganizationMode` Enum & CLI Integration

**Files:**
- Modify: `src/types.rs`
- Modify: `src/main.rs`
- Test: `tests/test_types.rs`

**Interfaces:**
- Produces: `pub enum OrganizationMode` with `InPlaceFileRenameToDescriptiveNames` (default) and `HierarchicalHardlinkHierarchy`.

- [ ] **Step 1: Write the failing test**
  Add unit tests in `tests/test_types.rs` verifying default mode, clap string parsing (`"in-place"`, `"hierarchy"`), and serialization roundtrip.
- [ ] **Step 2: Run test to verify it fails**
  Run `cargo test --test test_types` and verify `OrganizationMode` not found.
- [ ] **Step 3: Implement `OrganizationMode` in `src/types.rs` & wire `--mode` into `OrganizeArgs` in `src/main.rs`**
  Derive `clap::ValueEnum`, `Serialize`, `Deserialize`, `Default`. Add `--mode` option to `OrganizeArgs` defaulting to `in-place`.
- [ ] **Step 4: Run test to verify it passes**
  Run `cargo test --test test_types`.
- [ ] **Step 5: Commit**
  `git add src/types.rs src/main.rs tests/test_types.rs && git commit -m "feat(types): add OrganizationMode enum with in-place default"`

---

### Task 2: In-Place Filename Formatting & Sidecar Renamer

**Files:**
- Create: `src/organizer/inplace.rs`
- Modify: `src/organizer/mod.rs`
- Test: `tests/test_inplace_rename.rs`

**Interfaces:**
- Produces:
  - `pub fn format_inplace_filename(stem: &str, species: &str, date: Option<&str>, location: Option<&str>, ext: &str) -> String`
  - `pub fn execute_inplace_bundle_rename(bundle: &AssetBundle, new_stem: &str) -> Result<Vec<(PathBuf, PathBuf)>>`
  - `pub fn is_already_inplace_named(stem: &str, species: &str) -> bool`

- [ ] **Step 1: Write the failing test**
  In `tests/test_inplace_rename.rs`, test:
  1. Primary + sidecar (`.xmp`, `.CR2.xmp`) + companion JPEG renamed to matching stems.
  2. Date and location appended when present, cleanly omitted when absent.
  3. Idempotency: `is_already_inplace_named` returns true if already suffixed.
- [ ] **Step 2: Run test to verify it fails**
  Run `cargo test --test test_inplace_rename`.
- [ ] **Step 3: Implement in `src/organizer/inplace.rs`**
  Implement format string `<stem>_<Species>[_<Date>][_<Location>].<ext>`, sidecar discovery and lockstep `std::fs::rename`, and idempotency checking. Export from `src/organizer/mod.rs`.
- [ ] **Step 4: Run test to verify it passes**
  Run `cargo test --test test_inplace_rename`.
- [ ] **Step 5: Commit**
  `git add src/organizer/inplace.rs src/organizer/mod.rs tests/test_inplace_rename.rs && git commit -m "feat(organizer): implement in-place renaming and sidecar synchronization"`

---

### Task 3: SQLite Manifest Support for In-Place Renames & Reversible `clean`

**Files:**
- Modify: `src/db/mod.rs`
- Modify: `src/organizer/mod.rs`
- Test: `tests/test_clean_rollback.rs`

**Interfaces:**
- Produces:
  - `ManifestDb::record_inplace_rename(run_id, original_path, new_path) -> Result<()>`
  - `ManifestDb::rollback_inplace_renames(run_id) -> Result<usize>`
  - Safe `clean_run_hardlinks` / `clean_run_inplace`: reverts renames for in-place mode without deleting original media files.

- [ ] **Step 1: Write the failing test**
  In `tests/test_clean_rollback.rs`, create a temporary directory with mock RAW photos, perform in-place rename, record in DB, call rollback, and verify original filenames are 100% restored and content intact.
- [ ] **Step 2: Run test to verify it fails**
  Run `cargo test --test test_clean_rollback`.
- [ ] **Step 3: Implement rollback in `src/db/mod.rs` and `src/organizer/mod.rs`**
  Add `operation_type` column to operations table in SQLite. Implement `rollback_inplace_renames` reversing `std::fs::rename(current, original)`.
- [ ] **Step 4: Run test to verify it passes**
  Run `cargo test --test test_clean_rollback`.
- [ ] **Step 5: Commit**
  `git add src/db/mod.rs src/organizer/mod.rs tests/test_clean_rollback.rs && git commit -m "feat(db): record in-place renames and implement safe rollback clean"`

---

### Task 4: Standalone HTML Catalog Generator (`catalog.html`)

**Files:**
- Create: `src/catalog/mod.rs`
- Create: `src/catalog/template.rs`
- Modify: `src/lib.rs`
- Test: `tests/test_catalog.rs`

**Interfaces:**
- Produces:
  - `pub fn generate_html_catalog(trip_name: &str, items: &[ClassifiedBundleItem], mode: OrganizationMode, output_path: &Path) -> Result<()>`
  - Embedded client-side eBird taxonomy search, summary metrics, image grid, and mislabel correction modal generating `mv` commands.

- [ ] **Step 1: Write the failing test**
  In `tests/test_catalog.rs`, pass a slice of `ClassifiedBundleItem`s and verify `catalog.html` is generated on disk, contains valid HTML, embeds JSON stats, and includes the client-side JavaScript for autocomplete and `mv` command generation.
- [ ] **Step 2: Run test to verify it fails**
  Run `cargo test --test test_catalog`.
- [ ] **Step 3: Implement catalog generator in `src/catalog/`**
  Build the HTML generator with responsive cards, cloud-eviction `onerror` fallback tiles, summary badges, eBird taxonomy autocomplete list, and copyable `mv` shell commands.
- [ ] **Step 4: Run test to verify it passes**
  Run `cargo test --test test_catalog`.
- [ ] **Step 5: Commit**
  `git add src/catalog/ src/lib.rs tests/test_catalog.rs && git commit -m "feat(catalog): add standalone offline HTML catalog generator"`

---

### Task 5: End-to-End CLI Pipeline Integration

**Files:**
- Modify: `src/main.rs`
- Modify: `README.md`
- Test: `tests/test_cli_e2e.rs`

**Interfaces:**
- Connects `OrganizeArgs::mode` to the pipeline, invoking in-place renames by default, writing `catalog.html` and `.avicull/manifest.sqlite` into `--input`, and updating help documentation.

- [ ] **Step 1: Write the failing test**
  In `tests/test_cli_e2e.rs`, test `avicull organize --input <dir>` runs in-place mode by default, renames files in `<dir>`, and produces `<dir>/catalog.html`.
- [ ] **Step 2: Run test to verify it fails**
  Run `cargo test --test test_cli_e2e test_cli_e2e_inplace_default`.
- [ ] **Step 3: Wire in-place pipeline and catalog creation in `src/main.rs`**
  Execute `inplace_rename` and `generate_html_catalog` in `Commands::Organize`.
- [ ] **Step 4: Run test to verify it passes**
  Run `cargo test --test test_cli_e2e`.
- [ ] **Step 5: Commit**
  `git add src/main.rs README.md tests/test_cli_e2e.rs && git commit -m "feat(cli): wire default in-place organization mode and catalog generation"`

---

### Task 6: Full Verification, Documentation & PR Creation

**Files:**
- Modify: `README.md`
- Modify: `src/help.rs`
- Test: All suites (`cargo test`, `pytest`, `cargo fmt`, `cargo clippy`)

- [ ] **Step 1: Update documentation and CLI guide**
  Update `README.md` and `src/help.rs` (`organize` topic) explaining `--mode in-place` (default) vs `--mode hierarchy`, and `catalog.html`.
- [ ] **Step 2: Run formatting & linter**
  Run `cargo fmt --check` and `cargo clippy -- -D warnings`.
- [ ] **Step 3: Run complete test suite**
  Run `cargo test`, `pytest tests/test_build_gazetteer.py`, `python3 -m unittest discover -s tests_python`.
- [ ] **Step 4: Commit and push**
  Commit any doc changes, push to branch, and create GitHub Pull Request linked to Issue #5.
