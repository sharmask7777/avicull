# Durable SQLite Manifest and User Corrections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable portable SQLite deserialization across machines/users and durable user classification overrides via manual filesystem moves, CLI correction commands, and HTML catalog sync.

**Architecture:** Extend SQLite manifest schema with relative paths and override tracking; upgrade `ExistingBundleMap` to index bundles by `(trip_folder, original_stem)` and relative path; implement filesystem move reconciliation during scanning/organization; add `avicull correct` subcommand and update HTML catalog batch generator.

**Tech Stack:** Rust 2021, `rusqlite`, `clap`, `serde_json`, POSIX APFS hard links.

**Spec:** `docs/superpowers/specs/2026-10-26-durable-manifest-corrections-design.md`

## Global Constraints
- Strict Source Immutability in hierarchy mode: never modify or unlink files in `--input`.
- Inode parity & zero-byte payload invariants for hard links in output tree.
- Backward compatibility with existing `manifest.sqlite` schemas.
- Generic architecture with no hardcoded trip names or personal folder paths.
- All code follows TDD (red-green-refactor) with unit and integration tests.

## Review Focus
1. Bundle matching on cross-machine transfer: Inodes and absolute paths change completely, but `ExistingBundleMap` must match every existing bundle without running new AI inference.
2. User manual move on disk: File moved from `_Needs_Review/Anas_platyrhynchos` to `Anas_crecca` must be detected and promoted to `is_user_override = 1` with `RoutingTier::ConfirmedSpecies`.
3. In-place rename mode manual move/correction: When files were renamed or need correction, primary, companion images, and all compound sidecars (`.xmp`, `.JPG.xmp`) must stay grouped and update simultaneously.
4. `avicull correct` CLI subcommand: Handles non-existent bundle IDs gracefully with informative errors and updates both SQLite and disk.
5. Catalog script generation: `apply_corrections.sh` triggers `avicull correct` commands rather than only raw shell `mv`s, ensuring SQLite stays in sync.

---

### Task 1: Extend SQLite Schema & Types for Portable Relative Paths and Overrides

**Files:**
- Modify: `src/types.rs:40-75`
- Modify: `src/db.rs:25-145`, `src/db.rs:520-620`
- Test: `tests/test_db.rs`

**Interfaces:**
- Consumes: `BundleDecision`, `AssetBundle`
- Produces: `BundleDecision.is_user_override: bool`, `AssetBundle.relative_path: Option<PathBuf>`, `ExistingBundleMap.get_by_portable_key(...)`

- [ ] **Step 1: Write the failing test for schema migration and portable bundle retrieval**

In `tests/test_db.rs`, write `test_manifest_db_portable_bundle_lookup_and_override`:
```rust
#[test]
fn test_manifest_db_portable_bundle_lookup_and_override() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    let mut bundle = dummy_bundle();
    bundle.relative_path = Some(PathBuf::from("TripA/DSC_0001.JPG"));
    let mut decision = dummy_decision();
    decision.is_user_override = true;

    db.record_run("run_1", "/input", "/output", false, "{}").unwrap();
    db.record_bundle("run_1", &bundle, &decision).unwrap();

    let map = db.get_existing_bundle_map().unwrap();
    // Inode changed (e.g. simulated different machine)
    bundle.source_inode = 999999;
    let found = map.get_by_bundle(&bundle);
    assert!(found.is_some());
    assert!(found.unwrap().0.is_user_override);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_db test_manifest_db_portable_bundle_lookup_and_override`
Expected: FAIL due to missing fields `is_user_override` / `relative_path` or lookup failure.

- [ ] **Step 3: Implement fields and schema upgrade in `src/types.rs` and `src/db.rs`**

- In `src/types.rs`, add `is_user_override: bool` (with `#[serde(default)]`) to `BundleDecision`, and `relative_path: Option<PathBuf>` (with `#[serde(default)]`) to `AssetBundle`.
- In `src/db.rs`, alter `bundles` table if missing columns: `is_user_override INTEGER NOT NULL DEFAULT 0` and `relative_path TEXT`.
- Update `ExistingBundleMap` to maintain `by_portable_key: HashMap<(String, String), (BundleDecision, AssetBundle)>` (keyed by `(trip_folder_name, original_stem)`) and `by_relative_path: HashMap<PathBuf, (BundleDecision, AssetBundle)>`.
- Update `get_by_bundle(&self, bundle: &AssetBundle)` to check `by_id`, then `by_inode`, then `by_relative_path`, then `by_portable_key`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_db`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/types.rs src/db.rs tests/test_db.rs
git commit -m "feat(db): add portable bundle keys and user override tracking"
```

---

### Task 2: Populate Relative Path during Scanning and Support Cross-Machine Deserialization

**Files:**
- Modify: `src/scanner.rs:160-318`
- Modify: `src/main.rs:825-865`
- Test: `tests/test_portable_deserialization.rs`

**Interfaces:**
- Consumes: `scan_input_tree_with_skip`
- Produces: `AssetBundle.relative_path` populated relative to `input_dir`

- [ ] **Step 1: Write the failing integration test for cross-machine deserialization**

Create `tests/test_portable_deserialization.rs`:
```rust
#[test]
fn test_cross_machine_portable_deserialization() {
    let tmp = tempfile::tempdir().unwrap();
    let machine_a_root = tmp.path().join("machine_a");
    let machine_b_root = tmp.path().join("machine_b");

    // Create photos in machine_a, run organize
    // Copy output folder to machine_b with different root path
    // Run avicull on machine_b without AI models
    // Verify 100% cache hit without re-running inference
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_portable_deserialization`
Expected: FAIL

- [ ] **Step 3: Implement relative path calculation in `src/scanner.rs`**

- In `scan_input_tree_with_skip`, calculate `primary_image.strip_prefix(&can_input)` and assign to `bundle.relative_path`.
- When matching against `existing_bundle_map` in `src/main.rs`, verify that bundles match by portable relative path and `(trip_folder_name, original_stem)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_portable_deserialization`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/scanner.rs src/main.rs tests/test_portable_deserialization.rs
git commit -m "feat(scanner): populate relative path for portable bundle matching"
```

---

### Task 3: Filesystem Move Reconciliation (Detect Disk Moves and Record Durable Overrides)

**Files:**
- Create: `src/organizer/reconcile.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/db.rs`
- Modify: `src/main.rs`
- Test: `tests/test_reconcile_moves.rs`

**Interfaces:**
- Consumes: `ExistingBundleMap`, `output_root`
- Produces: `reconcile_manual_disk_moves(&output_root, &db, &existing_bundle_map) -> Result<usize>`

- [ ] **Step 1: Write the failing test for detecting manual filesystem moves**

Create `tests/test_reconcile_moves.rs`:
```rust
#[test]
fn test_reconcile_manual_species_directory_move() {
    // 1. Setup mock organize run where photo DSC_0001 is placed in birds_hierarchy/_Needs_Review/Mallard
    // 2. User manually moves DSC_0001 and its sidecar to birds_hierarchy/Northern_Pintail/
    // 3. Run reconciliation
    // 4. Check that SQLite bundles table is updated:
    //    assigned_species_slug == "Northern_Pintail", routing_tier == ConfirmedSpecies,
    //    final_confidence == 1.0, is_user_override == 1
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_reconcile_moves`
Expected: FAIL

- [ ] **Step 3: Implement `reconcile_manual_disk_moves` in `src/organizer/reconcile.rs`**

- Traverse `output_root` (skipping `.avicull`).
- For each photo found, extract original stem.
- Match to existing bundle in SQLite.
- Determine the folder's species slug from its parent directory (e.g. `birds_hierarchy/Northern_Pintail` -> `Northern_Pintail`).
- If current folder species differs from `existing_decision.assigned_species_slug`:
  - Update `bundles` table in SQLite with `assigned_species_slug = folder_slug`, `routing_tier = 'ConfirmedSpecies'`, `final_confidence = 1.0`, `is_user_override = 1`.
  - Update `hardlink_operations` record destination path.
- In `src/main.rs`, call reconciliation before partitioning new vs cached items.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_reconcile_moves`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/organizer/reconcile.rs src/organizer/mod.rs src/db.rs src/main.rs tests/test_reconcile_moves.rs
git commit -m "feat(reconcile): detect manual disk moves and record user overrides"
```

---

### Task 4: CLI Command `avicull correct` for Direct Classification Overrides

**Files:**
- Modify: `src/main.rs` (clap CLI definitions and subcommand handler)
- Modify: `src/organizer/mod.rs`
- Modify: `src/db.rs`
- Test: `tests/test_cli_correct.rs`

**Interfaces:**
- CLI: `avicull correct --input <INPUT> [--output <OUTPUT>] --file <FILENAME_OR_STEM> --species <SPECIES_SLUG_OR_NAME>`

- [ ] **Step 1: Write the failing test for `avicull correct` subcommand**

Create `tests/test_cli_correct.rs`:
```rust
#[test]
fn test_cli_correct_subcommand_updates_manifest_and_hardlink() {
    // 1. Run avicull organize on dummy photos
    // 2. Call avicull correct --file "DSC_0001" --species "Painted Stork"
    // 3. Verify SQLite record has is_user_override = 1, slug = "Painted_Stork"
    // 4. Verify hardlink exists under birds_hierarchy/Painted_Stork/
    // 5. Verify catalog.html is regenerated with new species
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_cli_correct`
Expected: FAIL

- [ ] **Step 3: Implement `CorrectArgs` and handler in `src/main.rs`**

- Add `Correct(CorrectArgs)` to `Commands` enum in `src/main.rs`.
- Implement `execute_correct(args: CorrectArgs)`:
  - Open `.avicull/manifest.sqlite`.
  - Locate bundle by matching `args.file` against `bundle_id`, `original_stem`, or primary image filename.
  - Sanitize species name/slug via `sanitize_species_slug`.
  - Move hardlink (or rename in-place file and sidecars) to destination.
  - Update SQLite `bundles` and `hardlink_operations`.
  - Regenerate `catalog.html` with updated counts.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_cli_correct`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/organizer/mod.rs tests/test_cli_correct.rs
git commit -m "feat(cli): add 'avicull correct' command for direct manual corrections"
```

---

### Task 5: Integrate HTML Catalog Corrections with `avicull correct`

**Files:**
- Modify: `src/catalog/template.rs`
- Test: `tests/test_catalog_corrections.rs`

**Interfaces:**
- Consumes: Catalog correction modal in `src/catalog/template.rs`
- Produces: `apply_corrections.sh` executing `avicull correct` to guarantee SQLite updates

- [ ] **Step 1: Write the test verifying `apply_corrections.sh` script format**

In `tests/test_catalog_corrections.rs`:
```rust
#[test]
fn test_catalog_template_generates_avicull_correct_commands() {
    // Verify template javascript produces `avicull correct --file ... --species ...`
    // commands so executing the script syncs the SQLite database.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_catalog_corrections`
Expected: FAIL

- [ ] **Step 3: Update `src/catalog/template.rs` command generator**

- Update `buildMoveCommands` in `template.rs`:
  - Output `avicull correct --file "${activeModalItem.original_stem}" --species "${newSlug}"` alongside or as the primary correction command in `apply_corrections.sh`.
- Update catalog modal help text.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_catalog_corrections`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/catalog/template.rs tests/test_catalog_corrections.rs
git commit -m "feat(catalog): sync apply_corrections.sh with avicull correct"
```

---

### Task 6: Full Integration & Real Sample Smoke Test

**Files:**
- Test: `tests/test_spec_conformance.rs`
- Run: `cargo test`

- [ ] **Step 1: Run complete test suite**

Run: `cargo test`
Expected: All unit and integration tests PASS.

- [ ] **Step 2: Run clippy and formatting checks**

Run:
```bash
cargo fmt -- --check
cargo clippy -- -D warnings
```
Expected: Zero warnings.

- [ ] **Step 3: Commit all remaining cleanups**

```bash
git commit --allow-empty -m "chore: complete durable sqlite manifest and manual corrections"
```
