# Strictly Incremental Catalog Generation, State-View Decoupling & Fast Re-classification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate hours/days of execution overhead on remote NAS shares by making `avicull organize-all` strictly incremental, unhiding catalog assets to `avicull_catalog_files`, defusing the 2.6GB HTML bomb via dual thumbnail strategy and lazy loading, decoupling view rendering via `render-catalog`, and providing fast software-only re-classification.

**Architecture:** Replace file walks with deep directory-only signature probes ($\max(\text{mtime})$ + $\sum(\text{entries})$) for $< 2\text{ms}$ child skips. Replace parent `run_organize` with `run_parent_rollup` using local NVMe SQLite staging and subdirectory-aware preview links (`{child}/avicull_catalog_files/previews/{stem}.jpg`). Suppress embedded base64 in rollups (shrinking HTML from 2.6GB to 28MB) with `loading="lazy"` and IntersectionObserver. Add `render-catalog` for pure view generation and `reclassify --reclassify-mode fast` for software-only re-scoring from stored visual embeddings.

**Tech Stack:** Rust (1.80+), SQLite (`rusqlite`), Rayon, `walkdir`, Clap, HTML5/Tailwind/CSS, Playwright (TypeScript).

**Spec:** `docs/superpowers/specs/2026-11-15-incremental-catalog-and-view-decoupling-design.md`

## Global Constraints

- `--mode in-place` remains the default and preferred mode: input archive is strictly read-only (`AGENTS.md` Invariant 1).
- Recursive scans of `--input` must ALWAYS skip `birds_hierarchy`, `avicull_catalog_files`, `.avicull`, `*.photoslibrary`, `.*`, and `@eaDir` (`AGENTS.md` Invariant 5).
- TDD is mandatory: **NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST** (`AGENTS.md` Invariant 2A).
- Zero warnings on `cargo clippy --all-targets --all-features -- -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- No `unsafe` code (`unsafe_code = "deny"`); no `unwrap` in production code (`unwrap_used = "deny"`).
- Peak memory during parent rollup of 100k photos must not exceed 350 MB RSS.

## Review Focus

1. **Nested Subdirectory Addition**: Adding a photo inside `Trip/Day1/CameraA/` must be detected by the directory-only signature probe without false-negative skips.
2. **Zero RAW Reads in Parent Rollup**: Running parent rollup with RAW files permissions set to `000` (or deleted) must succeed without error.
3. **Rollup HTML Size Capping**: Parent rollup of 100k items must suppress base64 data URIs, keeping HTML under 35 MB instead of 2.6 GB.
4. **Lazy Loading of Previews**: Previews in `catalog.html` must carry `loading="lazy"` and be deferred until scrolled into viewport.
5. **Fast Software Reclassification**: Reclassifying with updated confidence threshold must use stored embeddings in SQLite with 0 neural inference and 0 image reads.

---

### Task 1: Unhide Catalog Directory (`avicull_catalog_files`) & Traversal Scanners

**Files:**
- Create: `tests/test_suite/test_catalog_files_dir.rs`
- Modify: `src/types.rs:840-860`
- Modify: `src/catalog/mod.rs:320-360, 610-630, 900-940`
- Modify: `src/organizer/batch.rs:25-35`
- Modify: `src/scanner.rs:190-210, 510-545`
- Modify: `src/main.rs:510-565`
- Modify: `src/help.rs:100-140`

- [ ] **Step 1 (RED): Write failing test for `avicull_catalog_files` resolution and scanner skipping**
  Create `tests/test_suite/test_catalog_files_dir.rs`. Test that:
  - Default catalog directory name is `avicull_catalog_files`.
  - Directory traversal skips `avicull_catalog_files` and legacy `.avicull`.
  - Preview file resolution correctly targets `avicull_catalog_files/previews/{stem}.jpg`.
  - Existing `.avicull/manifest.sqlite` is discovered as backward-compatible fallback.

- [ ] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_catalog_files_dir` and verify failure.

- [ ] **Step 3 (GREEN): Implement `CATALOG_FILES_DIR` constant and update path resolvers**
  Define `pub const CATALOG_FILES_DIR: &str = "avicull_catalog_files";` in `src/catalog/mod.rs`.
  Update `should_skip_directory` in `src/organizer/batch.rs` and `should_skip_dir` in `src/scanner.rs` to skip `avicull_catalog_files`.
  Update `resolve_operating_root` and `resolve_manifest_db_path` in `src/main.rs` to prioritize `avicull_catalog_files` with `.avicull` fallback.

- [ ] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_catalog_files_dir` and confirm all tests pass.

- [ ] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(storage): unhide catalog assets directory to avicull_catalog_files"`.

---

### Task 2: Directory-Only Walk & Composite Signature Change Detection

**Files:**
- Create: `src/scanner/probe.rs`
- Modify: `src/scanner.rs:1-30`
- Modify: `src/db.rs:150-220, 600-650`
- Modify: `src/organizer/batch.rs:160-210`
- Modify: `src/organizer/engine.rs:860-910`
- Create: `tests/test_suite/test_incremental_skip.rs`

- [ ] **Step 1 (RED): Write failing tests for directory change detection**
  Create `tests/test_suite/test_incremental_skip.rs`. Test:
  - Fresh directory requires processing.
  - Completed directory with matching signature returns `UpToDate` in $< 5\text{ms}$.
  - Adding a file into nested `Trip/Day1/CameraA/` is detected via composite signature ($\max(\text{mtime})$, $\sum(\text{entries})$).
  - `organize-all` logs `Skipped` and bypasses `run_organize` on unchanged child directories.

- [ ] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_incremental_skip` and verify failure.

- [ ] **Step 3 (GREEN): Implement `DirectorySignature` and `directory_sync_state` SQLite table**
  Create `src/scanner/probe.rs` with `compute_directory_signature` using `walkdir::WalkDir` filtered to `is_dir()`.
  Add `directory_sync_state` table to `src/db.rs` with getter and setter methods.
  Integrate `probe_directory_change_status` in `src/organizer/batch.rs` to skip up-to-date child folders.
  Integrate fast skip in `src/organizer/engine.rs` (`run_organize`).

- [ ] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_incremental_skip` and verify all tests pass.

- [ ] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(scanner): implement deep directory-only signature probe for instant skip"`.

---

### Task 3: Dual Thumbnail Strategy & HTML Lazy Loading

**Files:**
- Modify: `src/catalog/mod.rs:350-410, 620-740`
- Modify: `src/catalog/template.rs:400-550`
- Create: `tests/test_suite/test_dual_thumbnails.rs`

- [ ] **Step 1 (RED): Write failing tests for thumbnail suppression and lazy loading**
  Create `tests/test_suite/test_dual_thumbnails.rs`. Test:
  - Parent rollup items have `embedded_thumb_data_uri == None`.
  - Preview URLs in parent catalog link to `{child_subdir}/avicull_catalog_files/previews/{stem}.jpg`.
  - Generated HTML contains `loading="lazy"` on gallery image tags.
  - JSON payload size for 1,000 rollup items is $< 300\text{ KB}$ (suppressed) vs $> 25\text{ MB}$ (embedded).

- [ ] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_dual_thumbnails` and verify failure.

- [ ] **Step 3 (GREEN): Implement `ThumbnailEmbeddingPolicy` and template lazy loading**
  In `src/catalog/mod.rs`, add `ThumbnailEmbeddingPolicy` (Auto, Suppressed, Forced).
  In `generate_html_catalog`, enforce `Suppressed` for parent rollups or when `--no-thumbnails` is passed.
  In `src/catalog/template.rs`, add `loading="lazy"` and decoding attributes to cards, and add an IntersectionObserver observer hook in the virtualized grid JS.

- [ ] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_dual_thumbnails` and verify pass.

- [ ] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(catalog): implement dual thumbnail strategy and lazy-loaded previews"`.

---

### Task 4: Zero-Image-I/O Parent Rollup & Local NVMe SQLite Staging

**Files:**
- Modify: `src/db.rs:250-320, 800-900`
- Modify: `src/organizer/batch.rs:140-235`
- Create: `tests/test_suite/test_parent_rollup.rs`

- [ ] **Step 1 (RED): Write failing test for zero-image-I/O parent rollup**
  Create `tests/test_suite/test_parent_rollup.rs`. Test:
  - Organize child directories.
  - Make all source RAW images inaccessible (`chmod 000`) or delete them.
  - Run parent rollup.
  - Assert parent rollup succeeds with 0 image read errors and outputs aggregated catalog.
  - Test pruning of child directory records when a child is removed.

- [ ] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_parent_rollup` and verify failure.

- [ ] **Step 3 (GREEN): Implement `parent_child_sync` and `run_parent_rollup`**
  Add `parent_child_sync` table in `src/db.rs` with `import_child_manifest` method.
  In `src/organizer/batch.rs`, implement `run_parent_rollup` staging SQLite in `/tmp/` before writing to output.
  Replace `run_organize(&parent_args)` with `run_parent_rollup`.
  Prune child records if a child directory is no longer present.

- [ ] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_parent_rollup` and verify pass.

- [ ] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(organizer): implement zero-image-IO parent rollup via SQLite staging"`.

---

### Task 5: Decoupled View Renderer CLI (`render-catalog`)

**Files:**
- Modify: `src/cli.rs:30-80, 280-320`
- Modify: `src/main.rs:30-90, 400-450`
- Modify: `src/organizer/mod.rs:700-750`
- Create: `tests/test_suite/test_render_catalog_cli.rs`

- [ ] **Step 1 (RED): Write failing tests for `render-catalog` command and `--view-only`**
  Create `tests/test_suite/test_render_catalog_cli.rs`. Test:
  - `avicull render-catalog --input <DIR>` re-renders `catalog.html` from SQLite in $< 1\text{s}$.
  - `avicull catalog` alias works identically.
  - `organize-all --view-only` re-renders all child and parent catalogs without scanning.

- [ ] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_render_catalog_cli` and verify failure.

- [ ] **Step 3 (GREEN): Wire `render-catalog` subcommand and `--view-only` flags**
  Add `Commands::RenderCatalog(RenderCatalogArgs)` with alias `catalog` in `src/cli.rs`.
  Add `--view-only` flag to `OrganizeArgs` and `OrganizeAllArgs`.
  Implement `render_catalog_standalone` in `src/organizer/mod.rs`.
  Wire execution in `src/main.rs`.

- [ ] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_render_catalog_cli` and verify pass.

- [ ] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(cli): add standalone render-catalog command and view-only update flags"`.

---

### Task 6: Fast Software Re-classification (`--reclassify-mode fast`)

**Files:**
- Modify: `src/cli.rs:90-140, 320-370`
- Modify: `src/organizer/engine.rs:860-930`
- Modify: `src/burst.rs:100-180`
- Modify: `src/main.rs:100-140`
- Create: `tests/test_suite/test_fast_reclassify.rs`

- [x] **Step 1 (RED): Write failing test for fast software re-classification**
  Create `tests/test_suite/test_fast_reclassify.rs`. Test:
  - Populate manifest with stored visual embeddings and posteriors.
  - Delete raw image files.
  - Run reclassify with `--reclassify-mode fast --min-confidence 0.80`.
  - Assert species routing tiers update in $< 500\text{ms}$ with 0 RAW reads.

- [x] **Step 2: Run test to verify failure**
  Run `cargo test --test test_suite test_fast_reclassify` and verify failure.

- [x] **Step 3 (GREEN): Implement `ReclassifyMode::Fast` software re-scoring**
  Add `ReclassifyMode` enum (`Fast` [default], `Forced`) in `src/cli.rs`.
  Implement `reclassify_from_cached_embeddings` in `src/organizer/engine.rs` / `src/burst.rs`:
  Loads stored embeddings/posteriors from SQLite, re-evaluates Bayesian priors and confidence thresholds, updates `bundle_decisions`, and regenerates `catalog.html`.

- [x] **Step 4: Run test to verify pass**
  Run `cargo test --test test_suite test_fast_reclassify` and verify pass.

- [x] **Step 5: Commit**
  Run `git add src/ tests/ && git commit -m "feat(reclassify): implement fast software re-scoring from stored embeddings"`.

---

### Task 7: Full Verification Suite & Mock Directory E2E

**Files:**
- Modify: `tests/test_mock_directory_structure_e2e.rs`
- Run: `./scripts/verify.sh`
- Run: `cd tests/e2e && npx playwright test`

- [ ] **Step 1: Update mock directory structure test**
  Update `tests/test_mock_directory_structure_e2e.rs` to verify that `avicull_catalog_files` is created, unchanged subdirectories are skipped on re-run, and parent rollup completes with zero errors.

- [ ] **Step 2: Run Playwright E2E visual catalog tests**
  Run `cd tests/e2e && npx playwright test` to verify card rendering, lazy loading, and parent-child navigation.

- [ ] **Step 3: Run complete 7-stage verification script**
  Run `./scripts/verify.sh` and ensure:
  1. `cargo fmt --check` passes.
  2. `cargo check --all-targets` passes.
  3. `cargo clippy --all-targets --all-features -- -D warnings` passes with 0 warnings.
  4. `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e` passes.
  5. Python verification suite passes.
  6. `cargo deny check` passes.
  7. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` passes with 0 warnings.

- [ ] **Step 4: Commit**
  Run `git add . && git commit -m "chore(verify): complete 7-stage verification pipeline for incremental catalog and view decoupling"`.
