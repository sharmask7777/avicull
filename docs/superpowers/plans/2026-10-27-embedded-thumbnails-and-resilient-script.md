# Embedded Thumbnails, Dynamic High-Res Swap, and Resilient Batch Scripting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bird_catalog.html` completely standalone and independent of `.avicull/` local sync by embedding base64 micro-thumbnails, dynamically upgrading to local high-res images if available on disk, gracefully handling deleted/modified files during reconciliation, and generating resilient `apply_corrections.sh` scripts that execute best-effort operations without halting on individual missing items.

**Architecture:** 
1. **Catalog Payload & Rendering**: Micro-thumbnails are extracted, downscaled, JPEG-encoded, base64 data-URI formatted, and embedded in the JSON payload of `bird_catalog.html`. The browser gallery instantly renders the embedded data URI, and if local resolution checks succeed or the full image loads in memory, dynamically swaps to high-res; if unreadable or evicted, it stays on the crisp embedded thumbnail.
2. **Reconciliation & Missing Files Resilience**: Scanning and reconciliation detect deleted or modified files gracefully without aborting the run. Missing bundle references in SQLite are purged or reconciled.
3. **Resilient Batch Correction Scripts**: Generated `apply_corrections.sh` commands run per-item with individual status reporting and safe exit-code handling so that if one file was moved or deleted outside `avicull`, remaining batch operations still succeed, ending with summary diagnostics of applied vs skipped actions.

**Tech Stack:** Rust 2021, Cargo, `image` crate (downsampling & JPEG encoding), `serde`, `rusqlite`, JavaScript (DOM, Image `onload` / `onerror`), Bash.

**Spec:** Issue #12: `.avicull directory requires syncing locally for the bird_catalog.html to render correctly even for cloud storage data`.

## Global Constraints
- Pure Rust base64 encoding without external network or heavy unnecessary dependencies.
- Zero extra disk payload on APFS volumes; thumbnails embedded in HTML must be micro-compressed (~8–15 KB per item) to keep catalog size compact.
- The input archive (`--input`) is strictly read-only during organization.
- Test-Driven Development (TDD) mandatory: failing test before any production code change.
- Never write procedural AI meta-comments (`// Step 1...`).

## Review Focus
- Memory consumption when processing large sets of photos for base64 thumbnails.
- Browser behavior when switching from base64 thumbnail to local asset.
- Handling when a photo bundle on disk has been deleted prior to running `apply_corrections.sh`.
- Catalog behavior when viewed on a machine where `.avicull/` is completely absent.
- Backward compatibility with existing SQLite manifests and bundle records.

---

### Task 1: Lightweight Base64 Micro-Thumbnail Generation in `src/catalog/`

**Files:**
- Modify: `src/catalog/mod.rs`
- Test: `tests/test_catalog.rs`

**Interfaces:**
- Produces: `embedded_thumb_data_uri: Option<String>` on `CatalogPayloadItem`
- Function: `pub fn generate_base64_micro_thumbnail(item: &CatalogItem, output_root: &Path) -> Option<String>`

- [ ] **Step 1: Write the failing test**
In `tests/test_catalog.rs`, write `test_catalog_embedded_base64_thumbnails()` verifying that when `generate_html_catalog` runs, `bird_catalog.html` embeds a valid `data:image/jpeg;base64,...` data URI for each item, and renders without `.avicull/previews/`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_catalog test_catalog_embedded_base64_thumbnails`
Expected: FAIL (field `embedded_thumb_data_uri` not found or test assertion fails).

- [ ] **Step 3: Implement base64 micro-thumbnail encoding**
Implement lightweight micro-thumbnail extraction and base64 encoding in `src/catalog/mod.rs` (300px max dimension, quality ~65 JPEG, or using existing preview bytes if already micro-sized), formatted as `data:image/jpeg;base64,<payload>`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_catalog test_catalog_embedded_base64_thumbnails`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/catalog/mod.rs tests/test_catalog.rs
git commit -m "feat(catalog): embed base64 micro-thumbnails in catalog payload"
```

---

### Task 2: Dynamic High-Res Replacement & Standalone Catalog Rendering in `template.rs`

**Files:**
- Modify: `src/catalog/template.rs`
- Test: `tests/test_catalog.rs`

**Interfaces:**
- Gallery card `<img>` displays `embedded_thumb_data_uri` immediately by default.
- If `is_local !== false` and `local_src` is specified, an off-screen Image probe or progressive load verifies availability and swaps `src` to `local_src` only upon successful load. If it fails or is evicted, it seamlessly stays on the base64 thumbnail.
- Completely remove runtime reliance on `.avicull/previews/` for gallery display.

- [ ] **Step 1: Write the failing test**
In `tests/test_catalog.rs`, write `test_catalog_standalone_no_avicull_dir()` verifying that when `bird_catalog.html` is generated, it contains inline data URIs in the JSON payload, initializes images with the data URI, and handles high-res swapping.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_catalog test_catalog_standalone_no_avicull_dir`
Expected: FAIL.

- [ ] **Step 3: Update `template.rs`**
Update JavaScript inside `template.rs`:
- Render `src="${item.embedded_thumb_data_uri || item.thumb_src}"` as the initial image source.
- Implement `attemptHighResSwap(img, item.local_src)`: creates an off-screen `new Image()`, sets `src = local_src`. On `onload`, updates `img.src = local_src`. On `onerror`, does nothing (keeps embedded thumb intact) and tags badge with cloud/evicted status.
- Modal photo summary uses the same robust fallback chain.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_catalog test_catalog_standalone_no_avicull_dir`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/catalog/template.rs tests/test_catalog.rs
git commit -m "feat(catalog): dynamic high-res image upgrade with embedded thumbnail baseline"
```

---

### Task 3: Resilient `apply_corrections.sh` Script Generation

**Files:**
- Modify: `src/catalog/template.rs`
- Test: `tests/test_catalog.rs`

**Interfaces:**
- Produces: `generateBatchScriptContent` in JavaScript generating best-effort shell commands.
- Behavior: Rather than dying immediately on `set -e` if a file was deleted or already renamed, wrap individual item corrections with error handling: log a warning for missing/failed files, increment error counter, continue executing remaining corrections, and exit with code 0 if all succeeded or code 1 with clear summary of failed items.

- [ ] **Step 1: Write the failing test**
In `tests/test_catalog.rs`, write `test_apply_corrections_resilient_to_missing_files()` verifying that when `apply_corrections.sh` contains a command for a nonexistent file followed by valid files, the valid operations execute and the script reports the failure without aborting mid-batch.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_catalog test_apply_corrections_resilient_to_missing_files`
Expected: FAIL.

- [ ] **Step 3: Implement resilient batch script generation**
In `src/catalog/template.rs`, update `buildMoveCommands` and `generateBatchScriptContent`:
- Wrap each file rename or `avicull correct` block in a resilient executor function `apply_single_correction`.
- Check if source exists before `mv` or `avicull correct`; if missing, print `[WARN] File not found: ... (skipping)` and continue.
- Keep track of total attempted, succeeded, and skipped.
- Re-run catalog organization at the end to reconcile all successful changes.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_catalog test_apply_corrections_resilient_to_missing_files`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/catalog/template.rs tests/test_catalog.rs
git commit -m "feat(catalog): resilient apply_corrections.sh with per-item error isolation"
```

---

### Task 4: Missing & Deleted File Reconciliation in Organizer

**Files:**
- Modify: `src/organizer/reconcile.rs`, `src/organizer/mod.rs`
- Test: `tests/test_reconcile_moves.rs`

**Interfaces:**
- Modifies: `reconcile_manual_disk_moves` and `reconcile_inplace_manual_renames`
- Behavior: Detects when a bundle previously in the manifest is no longer present on disk, purges stale bundle records or marks them without failing the reconciliation pass.

- [ ] **Step 1: Write the failing test**
In `tests/test_reconcile_moves.rs`, add `test_reconciliation_handles_deleted_files()` verifying that when an indexed file is deleted from disk and `organize` or `reconcile` runs, it completes successfully without crashing or halting, and catalog reflects current disk state.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_reconcile_moves test_reconciliation_handles_deleted_files`
Expected: FAIL.

- [ ] **Step 3: Implement deleted file handling in reconciliation**
In `src/organizer/reconcile.rs`, gracefully handle deleted files and ensure `ExistingBundleMap` and manifest updates handle missing inodes.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_reconcile_moves test_reconciliation_handles_deleted_files`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/reconcile.rs tests/test_reconcile_moves.rs
git commit -m "fix(reconcile): gracefully handle deleted files during manifest reconciliation"
```

---

### Task 5: Full Regression Testing & Multi-Agent Verification Crew

**Files:**
- Test: `tests/` (all integration and unit test suites)

- [ ] **Step 1: Run full test suite**
Run: `cargo test`
Expected: All tests PASS.

- [ ] **Step 2: Dispatch Verification Reviewers**
Review code changes across correctness, performance, idiomatic Rust, and spec conformance.

- [ ] **Step 3: Commit and push**
```bash
git commit -m "chore: bump version or finalize documentation updates"
```
