# Redefine In-Place Mode as Non-Destructive (No Rename) and Introduce In-Place-Rename Mode

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redefine the default `--mode in-place` organization mode as non-destructive (keeping input photo files completely untouched while generating `.avicull/` SQLite manifests and `catalog.html`), introduce an explicit `--mode in-place-rename` for cosmetic descriptive file renaming, and establish `in-place` as the preferred default mode in `AGENTS.md`.

**Architecture:** Split the `OrganizationMode` enum into `InPlace` (default: no rename, files untouched), `InPlaceRename` (opt-in cosmetic file rename with descriptive tokens), and `HierarchicalHardlinkHierarchy` (zero-copy APFS hardlink hierarchy). Update the execution pipeline in `src/main.rs`, correction routing in `src/organizer/correct.rs`, manifest mode serialization in `src/db.rs`, catalog metadata and UI script generators in `src/catalog/`, and CLI argument documentation in `src/cli.rs` and `src/help.rs`. Update `AGENTS.md` to canonize `in-place` as the default preferred mode.

**Tech Stack:** Rust (2021 edition), clap (derive `ValueEnum`), rusqlite, serde/serde_json, HTML5/ES6 photo catalog.

**Spec:** Direct user directive:
- Redefine `in-place` as a no rename mode (keep existing files untouched).
- Create a separate `in-place-rename` mode that performs descriptive file renaming.
- Renaming is purely cosmetic; everything in default `in-place` mode is self-contained in `.avicull/` and `catalog.html`.
- Update `AGENTS.md` so `in-place` is documented and enforced as the preferred default mode.

## Global Constraints

- **Strict Source Immutability in Default Mode**: `--input` files must NEVER be renamed, moved, or modified when running `--mode in-place`.
- **Zero Inode/Disk Duplication**: In `in-place` and `in-place-rename`, all assets operate directly in `--input` without creating duplicate tree structures.
- **TDD Mandatory**: Every code change must have a failing test written and verified before implementation code is written.
- **Strict Clippy & Rustdoc Gates**: Must pass `cargo clippy --all-targets --all-features -- -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- **Clean Codebase**: No procedural agent meta-comments (`// Step 1:`, etc.) in production code.

## Review Focus

1. **Default Mode Untouched Invariant**: In `--mode in-place`, verify that original source files and sidecars (`.CR2`, `.xmp`, `.jpg`) remain at their exact original file paths with unmodified file names and mtimes.
2. **In-Place-Rename Parity**: In `--mode in-place-rename`, verify that files and companion sidecars are renamed in lockstep with category, species slug, routing tier, date, and location tokens, matching the prior rename behavior.
3. **Database & Run Mode Provenance**: Verify that runs executed in `in-place` record mode `"in_place"` in SQLite `runs` table, while runs in `in-place-rename` record `"in_place_rename"`.
4. **Correction Script & CLI Invariant**: Verify that correcting an asset in `in-place` mode only updates SQLite and catalog records without touching files on disk, whereas correcting in `in-place-rename` renames the file on disk.
5. **Catalog HTML Interaction**: Verify that `catalog.html` generated in `in-place` mode correctly embeds paths to the original untouched filenames and generates correction shell commands with `--mode in-place`.

---

### Task 1: Update `OrganizationMode` Enum, CLI Parsing, and Database Deserialization

**Files:**
- Modify: `src/types.rs`
- Modify: `src/cli.rs`
- Modify: `src/db.rs`
- Test: `tests/test_suite/test_types.rs`

**Interfaces:**
- Consumes: `clap::ValueEnum`, `serde::Serialize`, `serde::Deserialize`
- Produces:
  - `OrganizationMode::InPlace` (default, CLI name `"in-place"`, alias `"no-rename"`)
  - `OrganizationMode::InPlaceRename` (CLI name `"in-place-rename"`, alias `"in-place-file-rename-to-descriptive-names"`)
  - `OrganizationMode::HierarchicalHardlinkHierarchy` (CLI name `"hierarchy"`, alias `"hardlink"`)
  - `OrganizationMode::InPlaceFileRenameToDescriptiveNames` as an associated constant / alias pointing to `OrganizationMode::InPlaceRename` for backwards compatibility.

- [ ] **Step 1: Write failing tests in `tests/test_suite/test_types.rs`**

```rust
#[test]
fn test_organization_mode_in_place_and_in_place_rename() {
    assert_eq!(OrganizationMode::default(), OrganizationMode::InPlace);

    assert_eq!(
        OrganizationMode::from_str("in-place", true).unwrap(),
        OrganizationMode::InPlace
    );
    assert_eq!(
        OrganizationMode::from_str("no-rename", true).unwrap(),
        OrganizationMode::InPlace
    );
    assert_eq!(
        OrganizationMode::from_str("in-place-rename", true).unwrap(),
        OrganizationMode::InPlaceRename
    );
    assert_eq!(
        OrganizationMode::from_str("in-place-file-rename-to-descriptive-names", true).unwrap(),
        OrganizationMode::InPlaceRename
    );
    assert_eq!(
        OrganizationMode::from_str("hierarchy", true).unwrap(),
        OrganizationMode::HierarchicalHardlinkHierarchy
    );

    assert_eq!(format!("{}", OrganizationMode::InPlace), "in-place");
    assert_eq!(format!("{}", OrganizationMode::InPlaceRename), "in-place-rename");
    assert_eq!(format!("{}", OrganizationMode::HierarchicalHardlinkHierarchy), "hierarchy");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_organization_mode_in_place`
Expected: FAIL due to missing `InPlace` variant and mismatched string values.

- [ ] **Step 3: Implement enum changes in `src/types.rs`, `src/cli.rs`, and `src/db.rs`**

1. In `src/types.rs`:
   - Redefine `OrganizationMode` with `InPlace`, `InPlaceRename`, and `HierarchicalHardlinkHierarchy`.
   - Implement `Display` returning `"in-place"`, `"in-place-rename"`, and `"hierarchy"`.
   - Maintain alias `pub const InPlaceFileRenameToDescriptiveNames: Self = Self::InPlaceRename;`
2. In `src/cli.rs`:
   - Update `OrganizeArgs.mode` default to `OrganizationMode::InPlace`.
   - Update doc comments for `--mode` to `[in-place | in-place-rename | hierarchy] (default: in-place)`.
3. In `src/db.rs`:
   - In `get_latest_run_mode()`, handle `"in_place"` / `"in-place"` -> `InPlace`, `"in_place_rename"` / `"in-place-rename"` -> `InPlaceRename`, and `"hierarchy"` -> `HierarchicalHardlinkHierarchy`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_organization_mode_in_place`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/types.rs src/cli.rs src/db.rs tests/test_suite/test_types.rs
git commit -m "feat(types): redefine in-place as non-destructive default and add in-place-rename mode"
```

---

### Task 2: Decouple Pipeline Execution: Non-Destructive In-Place vs In-Place-Rename

**Files:**
- Modify: `src/main.rs`
- Test: `tests/test_suite/test_inplace_no_rename.rs`
- Test: `tests/test_suite/test_cli_e2e.rs`

**Interfaces:**
- Consumes: `OrganizationMode`, `AssetBundle`, `BundleDecision`, `CatalogItem`
- Produces:
  - `InPlace` execution path: leaves source files untouched, records 0 file renames, passes original paths to `CatalogItem`.
  - `InPlaceRename` execution path: renames files on disk with descriptive slugs, records renames in SQLite, passes renamed paths to `CatalogItem`.

- [ ] **Step 1: Write failing test in `tests/test_suite/test_inplace_no_rename.rs`**

Write test verifying that running `organize` in default `in-place` mode:
1. Keeps `IMG_4821.CR2` and `IMG_4821.CR2.xmp` on disk under their original names.
2. Creates `.avicull/manifest.sqlite` with decisions.
3. Generates `catalog.html` referencing the original files.
4. Does NOT create any renamed files on disk.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_inplace_no_rename`
Expected: FAIL because `main.rs` currently renames files in `in-place`.

- [ ] **Step 3: Update `src/main.rs` execution logic**

1. Distinguish `OrganizationMode::InPlace` from `OrganizationMode::InPlaceRename`:
   - `let is_inplace_any = matches!(args.mode, OrganizationMode::InPlace | OrganizationMode::InPlaceRename);`
   - `let is_rename = matches!(args.mode, OrganizationMode::InPlaceRename);`
2. In reconciliation: only run `avicull::organizer::reconcile_inplace_manual_renames` if `is_rename` is true.
3. In run logging:
   ```rust
   let mode_str = match args.mode {
       OrganizationMode::InPlace => "in_place",
       OrganizationMode::InPlaceRename => "in_place_rename",
       OrganizationMode::HierarchicalHardlinkHierarchy => "hierarchy",
   };
   ```
4. In materialization:
   - If `args.mode == OrganizationMode::InPlace`:
     Skip `execute_inplace_bundle_rename_with_category`. Do not mutate filenames. Build `CatalogItem` with `bundle.primary_image`, `bundle.companion_images`, `bundle.sidecar_files`.
   - If `args.mode == OrganizationMode::InPlaceRename`:
     Execute `execute_inplace_bundle_rename_with_category`, record renames in SQLite, build `CatalogItem` with renamed paths.
   - If `args.mode == OrganizationMode::HierarchicalHardlinkHierarchy`:
     Materialize hard links into `birds_hierarchy/`.
5. Update `tests/test_suite/test_cli_e2e.rs` to reflect that default `in-place` keeps files untouched, and add a test case verifying `--mode in-place-rename` renames them.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_suite test_inplace_no_rename && cargo test --test test_suite test_cli_e2e`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/main.rs tests/test_suite/test_inplace_no_rename.rs tests/test_suite/test_cli_e2e.rs tests/test_suite/main.rs
git commit -m "feat(organize): keep files untouched in in-place mode and rename in in-place-rename mode"
```

---

### Task 3: Handle Corrections and Reconcile for In-Place vs In-Place-Rename

**Files:**
- Modify: `src/organizer/correct.rs`
- Modify: `src/main.rs`
- Test: `tests/test_suite/test_cli_correct.rs`

**Interfaces:**
- Consumes: `apply_bundle_correction_with_category(db, operating_root, mode, file, category, species, ...)`
- Produces:
  - For `InPlace`: updates database record and override flag (`is_user_override = 1`), leaves files on disk untouched.
  - For `InPlaceRename`: updates database and renames file on disk to new descriptive filename.
  - For `HierarchicalHardlinkHierarchy`: updates database and relocates hard links.

- [ ] **Step 1: Write failing test in `tests/test_suite/test_cli_correct.rs`**

Write test verifying that `avicull correct --mode in-place --category Bird --species "House_Sparrow" --file "IMG_1001"` updates the SQLite database override but leaves `IMG_1001.JPG` untouched on disk.
Verify that `avicull correct --mode in-place-rename --category Bird --species "House_Sparrow" --file "IMG_1001"` updates SQLite AND renames `IMG_1001.JPG` to `IMG_1001_Bird_House_Sparrow...jpg`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_correct_inplace_no_rename`
Expected: FAIL because `correct.rs` currently renames files in all in-place modes.

- [ ] **Step 3: Update `src/organizer/correct.rs` and `src/main.rs`**

1. In `src/organizer/correct.rs`:
   - Match on `OrganizationMode`:
     ```rust
     match mode {
         OrganizationMode::HierarchicalHardlinkHierarchy => {
             // Move hard links in birds_hierarchy
         }
         OrganizationMode::InPlaceRename => {
             // Rename files on disk in-place
         }
         OrganizationMode::InPlace => {
             // Files remain strictly untouched on disk.
             // Only database records (bundle override and classification) are updated.
         }
     }
     ```
2. In `src/main.rs` `execute_correct`:
   - Update mode defaulting:
     ```rust
     let mode = if let Some(m) = args.mode {
         m
     } else if let Ok(Some(m)) = db.get_latest_run_mode() {
         m
     } else if args.output.is_some() || db_path.to_string_lossy().contains("birds_hierarchy") {
         OrganizationMode::HierarchicalHardlinkHierarchy
     } else {
         OrganizationMode::InPlace
     };
     ```
   - For `InPlace` and `InPlaceRename`, set operating root to `--input` or parent of db.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_correct_inplace_no_rename`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/organizer/correct.rs src/main.rs tests/test_suite/test_cli_correct.rs
git commit -m "feat(correct): preserve untouched files in in-place correction and rename in in-place-rename"
```

---

### Task 4: Catalog Template, UI Script Generators, and Help Text Updates

**Files:**
- Modify: `src/catalog/mod.rs`
- Modify: `src/catalog/template.rs`
- Modify: `src/help.rs`
- Test: `tests/test_suite/test_catalog.rs`
- Test: `tests/test_suite/test_help.rs`

**Interfaces:**
- Consumes: `OrganizationMode`
- Produces:
  - `CATALOG_DATA.mode = "in-place"` | `"in-place-rename"` | `"hierarchy"`
  - Bash commands inside catalog modal generate `--mode in-place` or `--mode in-place-rename` matching the active mode.
  - Interactive help topics document `in-place` as default non-destructive mode and `in-place-rename` as cosmetic renaming.

- [ ] **Step 1: Write failing tests in `tests/test_suite/test_catalog.rs` and `tests/test_suite/test_help.rs`**

1. In `test_catalog.rs`:
   - Test that catalog generated with `OrganizationMode::InPlace` sets `CATALOG_DATA.mode = "in-place"`.
   - Test that catalog generated with `OrganizationMode::InPlaceRename` sets `CATALOG_DATA.mode = "in-place-rename"`.
   - Test that the shell script generated in `buildMoveCommandsForItem` outputs `--mode in-place-rename` when `CATALOG_DATA.mode === 'in-place-rename'`, and `--mode in-place` when `'in-place'`.
2. In `test_help.rs`:
   - Verify help output documents `--mode <MODE>` with `in-place (default, untouched files)`, `in-place-rename`, and `hierarchy`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --test test_suite test_catalog && cargo test --test test_suite test_help`
Expected: FAIL due to missing `in-place-rename` handling.

- [ ] **Step 3: Update `src/catalog/mod.rs`, `src/catalog/template.rs`, and `src/help.rs`**

1. In `src/catalog/mod.rs`:
   ```rust
   let mode_str = match mode {
       OrganizationMode::HierarchicalHardlinkHierarchy => "hierarchy",
       OrganizationMode::InPlaceRename => "in-place-rename",
       OrganizationMode::InPlace => "in-place",
   };
   ```
2. In `src/catalog/template.rs`:
   ```javascript
   const modeFlag = isHierarchy ? '--mode hierarchy' : (CATALOG_DATA.mode === 'in-place-rename' ? '--mode in-place-rename' : '--mode in-place');
   ```
3. In `src/help.rs`:
   Update `HelpTopic::Quickstart`, `HelpTopic::Organize`, `HelpTopic::All` to document:
   - `in-place` (default): non-destructive cataloging leaving photo files untouched.
   - `in-place-rename`: cosmetic descriptive file renaming.
   - `hierarchy`: zero-copy APFS hardlink hierarchy.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_suite test_catalog && cargo test --test test_suite test_help`
Expected: PASS

- [ ] **Step 5: Commit changes**

```bash
git add src/catalog/mod.rs src/catalog/template.rs src/help.rs tests/test_suite/test_catalog.rs tests/test_suite/test_help.rs
git commit -m "feat(catalog): support in-place and in-place-rename modes in catalog UI and help text"
```

---

### Task 5: Update AGENTS.md, Documentation, and Full Verification Suite

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Test: Full verification suite (`./scripts/verify.sh`)

**Interfaces:**
- Produces:
  - Canonical `AGENTS.md` guidelines establishing `in-place` as the preferred default mode.
  - Passing `./scripts/verify.sh` covering format, clippy, unit/integration tests, e2e mock tests, python tests, cargo deny, and rustdoc.

- [ ] **Step 1: Update `AGENTS.md`**

In `AGENTS.md`:
1. In Section 1 (Non-Negotiable Core Invariants):
   - Update Invariant 1 (Strict Source Immutability) and clarify how `in-place` mode guarantees this:
     - `in-place` is the default and preferred mode. It keeps source files strictly untouched; all metadata, thumbnails/previews, and HTML catalog are self-contained in `.avicull/` and `catalog.html`.
     - `in-place-rename` is an opt-in mode for cosmetic file renaming.
     - `hierarchy` is an opt-in mode for APFS zero-byte hardlink folder trees.
2. Ensure consistent terminology across Section 1, 2, and 5.

- [ ] **Step 2: Update `README.md`**

Update usage examples, CLI flags table, and quickstart in `README.md` to reflect `in-place` (default non-destructive) and `in-place-rename`.

- [ ] **Step 3: Run the full 7-stage verification script**

Run: `./scripts/verify.sh`
Confirm:
- `cargo fmt --check`: PASS
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS
- `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`: PASS
- Python verification suite (`scripts/verify_python.sh`): PASS
- `cargo deny check`: PASS
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: PASS

- [ ] **Step 4: Commit changes**

```bash
git add AGENTS.md README.md
git commit -m "docs: establish in-place as preferred default mode in AGENTS.md and README"
```
