# Advanced Corrections & In-Place Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement ambiguity detection, in-place move reconciliation, burst propagation, and fuzzy species suggestions for `avicull correct` and `avicull organize`.

**Architecture:** Extend `correct.rs` with multi-match resolution and burst propagation; extend `reconcile.rs` with in-place rename reconciliation; add taxonomy trigram suggestion in `correct.rs`.

**Tech Stack:** Rust 2021, `rusqlite`, `clap`, `std::io::IsTerminal`.

**Spec:** `docs/superpowers/specs/2026-10-26-advanced-corrections-design.md`

## Global Constraints
- Strict Source Immutability in hierarchy mode.
- Inode parity & zero-byte payloads in hierarchy mode; safe atomic renames in in-place mode.
- TDD is mandatory: Red -> Green -> Refactor for all tasks.
- No procedural AI meta-comments.

## Review Focus
1. Ambiguous match resolution: If 2 bundles match `--file DSC_0001`, ensure non-interactive fails with list of options, while interactive allows selection.
2. In-place rename reconciliation: Ensure companion and sidecars get renamed to match manually modified primary image.
3. Burst propagation: Single command cleanly updates all N bundles in burst in SQLite and on disk.
4. Fuzzy suggestions: eBird taxonomy lookup suggests correct spelling without breaking custom non-taxonomy slugs.

---

### Task 1: Ambiguity Detection & Disambiguation for `avicull correct`

**Files:**
- Modify: `src/organizer/correct.rs`
- Test: `tests/test_cli_correct.rs`

- [ ] **Step 1: Write failing test for ambiguous `--file` matching**
In `tests/test_cli_correct.rs`, write `test_cli_correct_ambiguous_file_fails_with_choices`:
Two trips both have `DSC_0001.JPG`. Running `avicull correct --file "DSC_0001"` non-interactively must fail with an error explicitly naming both options.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_cli_correct test_cli_correct_ambiguous_file_fails_with_choices`
Expected: FAIL.

- [ ] **Step 3: Implement multi-match collection and interactive/non-interactive disambiguation**
In `src/organizer/correct.rs`, collect all matching bundles in `matched_bundles: Vec<(BundleDecision, AssetBundle)>`. If length > 1, handle interactive selection or return error with relative paths.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_cli_correct`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/correct.rs tests/test_cli_correct.rs
git commit -m "feat(correct): add ambiguity detection and disambiguation for file queries"
```

---

### Task 2: Burst Propagation (`--propagate-burst` / `--burst`)

**Files:**
- Modify: `src/main.rs`
- Modify: `src/organizer/correct.rs`
- Test: `tests/test_cli_correct.rs`

- [ ] **Step 1: Write failing test for burst propagation**
In `tests/test_cli_correct.rs`, write `test_cli_correct_burst_propagation`:
A burst of 3 frames (`DSC_0001`, `DSC_0002`, `DSC_0003`) shares `burst_id`. Running `avicull correct --file DSC_0001 --species "Painted Stork" --burst` must update all 3 frames in SQLite and on disk.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_cli_correct test_cli_correct_burst_propagation`
Expected: FAIL.

- [ ] **Step 3: Implement burst propagation in `src/organizer/correct.rs` and `src/main.rs`**
Add `--propagate-burst` / `-b` flag to `CorrectArgs`. When active or confirmed, fetch all bundles in DB sharing `burst_id` and apply correction to each.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_cli_correct`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/main.rs src/organizer/correct.rs tests/test_cli_correct.rs
git commit -m "feat(correct): support burst propagation for photo reclassification"
```

---

### Task 3: In-Place Move & Rename Reconciliation

**Files:**
- Modify: `src/organizer/reconcile.rs`
- Modify: `src/main.rs`
- Test: `tests/test_reconcile_moves.rs`

- [ ] **Step 1: Write failing test for in-place mode rename reconciliation**
In `tests/test_reconcile_moves.rs`, write `test_reconcile_inplace_manual_filename_rename`:
Organize photos in in-place mode. Manually rename primary image `DSC_0001_Mallard.JPG` to `DSC_0001_Northern_Pintail.JPG`. Running `avicull organize` in in-place mode reconciles the user override in SQLite and renames companion/sidecars (`DSC_0001_Mallard.xmp` -> `DSC_0001_Northern_Pintail.xmp`).

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_reconcile_moves test_reconcile_inplace_manual_filename_rename`
Expected: FAIL.

- [ ] **Step 3: Implement in-place reconciliation in `src/organizer/reconcile.rs`**
Add `reconcile_inplace_manual_renames(operating_root: &Path, db: &ManifestDb, existing_bundle_map: &mut ExistingBundleMap) -> Result<usize>`. Call it in `src/main.rs` when `mode == InPlaceFileRenameToDescriptiveNames`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_reconcile_moves`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/reconcile.rs src/main.rs tests/test_reconcile_moves.rs
git commit -m "feat(reconcile): support in-place manual rename reconciliation"
```

---

### Task 4: Fuzzy Species Autocorrect & Typo Suggestions

**Files:**
- Modify: `src/organizer/correct.rs`
- Test: `tests/test_cli_correct.rs`

- [ ] **Step 1: Write failing test for fuzzy species suggestions**
In `tests/test_cli_correct.rs`, write `test_cli_correct_fuzzy_species_matching`:
Verify that running `correct` with typos or close matches resolves to the canonical species name or prompts.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_cli_correct test_cli_correct_fuzzy_species_matching`
Expected: FAIL.

- [ ] **Step 3: Implement fuzzy species matching in `src/organizer/correct.rs`**
Use `location::trigram_similarity` against loaded taxonomy names.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_cli_correct`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/correct.rs tests/test_cli_correct.rs
git commit -m "feat(correct): add fuzzy species typo detection and suggestion"
```

---

### Task 5: Documentation & Regression Suite

**Files:**
- Modify: `README.md`
- Run: `cargo test --all-targets`
- Run: `cargo fmt -- --check`
- Run: `cargo clippy --all-targets -- -D warnings`

- [ ] **Step 1: Run complete test suite and linters**
- [ ] **Step 2: Update README with `--burst` and in-place reconciliation details**
- [ ] **Step 3: Commit**
```bash
git add README.md
git commit -m "docs: document burst propagation and in-place reconciliation"
```
