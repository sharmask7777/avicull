# Extensible Multi-Taxa Classification & Semantic Embedding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement open-set multi-taxa classification (`Bird`, `Mammalia`, `Insecta`, `Scenery`) and persistent 512-dim visual embeddings to resolve misclassifications (Issue #21) and establish the zero-compute architectural foundation for general animal search (Issue #40).

**Architecture:** Extend BioCLIP taxonomy embeddings with mammal, insect, and scenery categories. Employ 2-stage hierarchical gating ($\mathbf{s}_{\text{cat}} = \mathbf{z} \cdot \mathbf{E}_{\text{cat}}^T$) in Python inference, store 512-dim visual embeddings as compact binary `BLOB`s in SQLite, migrate filenames and directory trees to a singular 2-level `<stem>_<Category>_<Subject>...` schema, and update the interactive HTML catalog with dynamic category filters.

**Tech Stack:** Rust (2021 edition), `rusqlite`, `image`, `rayon`, Python 3 (`open_clip`, `torch`, `safetensors`, `ultralytics`), SQLite, HTML/JS/CSS, Playwright.

**Spec:** [`docs/superpowers/specs/2026-11-04-extensible-taxa-classification-and-semantic-search-design.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/superpowers/specs/2026-11-04-extensible-taxa-classification-and-semantic-search-design.md)

---

## Global Constraints

- **Source Immutability**: Input files (`--input`) are strictly read-only. Never modify, move, or unlink them.
- **APFS Zero-Byte Hard Links**: Output tree uses `std::fs::hard_link` (`dst_meta.ino() == src_ino`, `st_nlink >= 2`).
- **Hardware & Memory Limit**: Max runtime memory footprint $< 1.5\text{ GB}$. Must execute seamlessly on 16GB M2 Pro MacBook.
- **TDD Requirement**: No production code without a failing test first.
- **Code Coverage Gate**: Minimum 80% line coverage gate on Python (`scripts/verify_python.sh`) and Rust (`cargo llvm-cov`).
- **Nomenclature**: Category tokens are strictly singular: `Bird`, `Mammalia`, `Insecta`, `Scenery`.
- **Backward Compatibility**: Fully recognize legacy `_No_Bird_Detected` and legacy single-level bird filenames (`DSC_0042_Painted_Stork.CR2`).

---

## Review Focus

1. **Legacy Filename Migration without Splintering**: Legacy 1-level bird files (e.g. `DSC_0042_Painted_Stork.CR2`) must not parse `Painted` as category and `Stork` as subject.
2. **Detector Zero-Crop Fallback**: When YOLO detects 0 boxes, BioCLIP must evaluate scenery vs low confidence, routing to `_No_Subject_Detected` with confidence 0.0, never guessing a random bird.
3. **SQLite BLOB Endianness & Dimension Safety**: The 512 `f32` embedding must serialize as exactly 2048 bytes (IEEE-754 little-endian) and deserialize safely across architectures.
4. **Non-Avian Geospatial Decoupling**: Regional checklists must not penalize valid non-avian taxa (mammals, insects, scenery) with avian checklist penalties.
5. **Interactive Catalog Filter State & Count Integrity**: Changing category pills in `bird_catalog.html` must update pagination, selection counts, and modal CLI generation without JavaScript runtime exceptions.

---

## File Structure & Module Responsibilities

| File | Responsibility |
| :--- | :--- |
| `scripts/build_taxonomy_embeddings.py` | Generates multi-taxa embedding matrices (`taxa_embeddings.safetensors`, `taxonomy.json`) covering Bird, Mammalia, Insecta, and Scenery. |
| `scripts/infer.py` | Executes 2-stage hierarchical gating, crops COCO animals/birds, and extracts normalized 512-dim visual embeddings. |
| `src/types.rs` | Defines `AssetCategory`, `TaxonPrediction`, `ClassifiedBundleItem` updates, and serialization. |
| `src/organizer/inplace.rs` | Formats in-place 2-level filenames: `<stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location>].<ext>`. |
| `src/organizer/reconcile.rs` | Parses 2-level filenames, distinguishes known category registry tokens, and promotes legacy v1 filenames. |
| `src/db.rs` | SQLite schema migration: `category TEXT NOT NULL DEFAULT 'Bird'` and `visual_embedding BLOB`. |
| `src/classifier.rs` | `TaxonClassifier` trait abstraction supporting arbitrary biological domains and visual embeddings. |
| `src/burst.rs` | Burst consensus clustering across multi-taxa items, preserving category assignments. |
| `src/organizer/mod.rs` | Hierarchy output builder routing to `<output>/<Category>/<Subject>/`. |
| `src/catalog/mod.rs` & `src/catalog/template.rs` | HTML catalog rendering with category filter pills, card badges, and unified search. |
| `tests/test_suite/test_extensible_taxa_guardrails.rs` | Architectural guardrail test suite ensuring future refactors cannot restrict domains back to birds only. |

---

### Task 1: Multi-Taxa Hierarchical Taxonomy & Embedding Builder

**Files:**
- Modify: `scripts/build_taxonomy_embeddings.py`
- Test: `tests_python/test_build_taxonomy_embeddings.py`

**Interfaces:**
- Produces: `TaxonomyEntry(species_code, scientific_name, common_name, category, aliases)`
- Produces: `models/taxonomy.json` and `models/species_embeddings.safetensors` supporting `Bird`, `Mammalia`, `Insecta`, `Scenery`.

- [ ] **Step 1: Write failing Python unit tests in `tests_python/test_build_taxonomy_embeddings.py`**
  - Test `TaxonomyEntry` serialization with `category` field defaulting to `"Bird"`.
  - Test parsing multi-taxa records including `"Mammalia"`, `"Insecta"`, and `"Scenery"`.
  - Test embedding prompt generation per category (`"a photo of a {common_name} ({scientific_name}), a type of mammal."`, etc.).
- [ ] **Step 2: Run test to verify failure**
  - Run: `uv run --with pytest pytest tests_python/test_build_taxonomy_embeddings.py -v`
  - Expected: FAIL with missing `category` or schema mismatch.
- [ ] **Step 3: Implement multi-taxa taxonomy parsing and prompt generation in `scripts/build_taxonomy_embeddings.py`**
  - Add `category: str = "Bird"` to `TaxonomyEntry`.
  - Ingest mammal (IUCN/iNat) and butterfly/insect taxonomy records alongside Cornell eBird taxonomy.
  - Generate category-specific BioCLIP text prompts.
- [ ] **Step 4: Run test to verify it passes**
  - Run: `uv run --with pytest pytest tests_python/test_build_taxonomy_embeddings.py -v`
  - Expected: PASS
- [ ] **Step 5: Commit**
  - `git add scripts/build_taxonomy_embeddings.py tests_python/test_build_taxonomy_embeddings.py`
  - `git commit -m "feat(taxonomy): support multi-taxa categories in taxonomy embedding builder"`

---

### Task 2: Two-Stage Hierarchical Inference Engine & Gating in `scripts/infer.py`

**Files:**
- Modify: `scripts/infer.py`
- Test: `tests_python/test_infer.py`

**Interfaces:**
- Consumes: `TaxonomyEntry` with `category` from Task 1.
- Produces: `InferencePrediction(category, species_code, common_name, scientific_name, confidence, embedding)`

- [ ] **Step 1: Write failing Python unit tests in `tests_python/test_infer.py`**
  - Test `classify_category_anchors` correctly identifies mammal, insect, bird, and scenery centroids.
  - Test open-set rejection: a butterfly embedding is rejected from birds and assigned to `Insecta`.
  - Test detector fallback: `crop is None` on scenery photo returns `Scenery` category instead of bird.
  - Test detector fallback: `crop is None` on ambiguous blur returns `_No_Subject_Detected` with `confidence = 0.0`.
- [ ] **Step 2: Run test to verify failure**
  - Run: `uv run --with pytest pytest tests_python/test_infer.py -v`
  - Expected: FAIL
- [ ] **Step 3: Implement 2-stage hierarchical gating and COCO animal detection in `scripts/infer.py`**
  - Precompute category centroid vectors $\mathbf{E}_{\text{cat}}$ for `Bird`, `Mammalia`, `Insecta`, `Scenery`.
  - Implement Stage 1 category gating with margin $\delta = 0.02$.
  - Expand YOLO detector candidate box selection to include COCO animal class IDs (`cat=15`, `dog=16`, `horse=17`, `sheep=18`, `cow=19`, `elephant=20`, `bear=21`, `zebra=22`, `giraffe=23`).
  - Return `category` and full 512-dim visual embedding in JSON results.
- [ ] **Step 4: Run test to verify it passes**
  - Run: `uv run --with pytest pytest tests_python/test_infer.py -v`
  - Expected: PASS
- [ ] **Step 5: Run Python verification suite**
  - Run: `./scripts/verify_python.sh`
  - Expected: PASS (ruff check, ruff format, mypy, pytest coverage $\ge 80\%$)
- [ ] **Step 6: Commit**
  - `git add scripts/infer.py tests_python/test_infer.py`
  - `git commit -m "feat(infer): add 2-stage hierarchical gating and open-set rejection"`

---

### Task 3: Domain Types & In-Place 2-Level Naming

**Files:**
- Modify: `src/types.rs`
- Modify: `src/organizer/inplace.rs`
- Modify: `src/organizer/reconcile.rs`
- Test: `tests/test_suite/test_inplace_rename.rs`
- Test: `tests/test_suite/test_reconcile_moves.rs`

**Interfaces:**
- Produces: `pub enum AssetCategory { Bird, Mammalia, Insecta, Scenery, Other(String) }`
- Produces: `format_inplace_filename(stem, category, subject, date, location, ext) -> String`
- Produces: `parse_inplace_filename(filename, original_stem) -> Option<(AssetCategory, String, RoutingTier, Option<String>, Option<String>)>`

- [ ] **Step 1: Write failing Rust tests in `tests/test_suite/test_inplace_rename.rs` and `test_reconcile_moves.rs`**
  - Test formatting: `format_inplace_filename("DSC_0042", "Bird", "white-throated_kingfisher", Some("20261102"), Some("Western_Ghats"), "CR2")` yields `DSC_0042_Bird_white-throated_kingfisher_20261102_Western_Ghats.CR2`.
  - Test formatting for mammals: `DSC_0105_Mammalia_malabar_giant_squirrel_20261102_Western_Ghats.CR2`.
  - Test formatting unclassified: `DSC_0300_No_Subject_Detected_20261102.CR2`.
  - Test round-trip parsing of 2-level names.
  - Test backward-compatibility parsing of legacy v1 files (`DSC_0042_Painted_Stork.CR2` promoted to `Bird` category).
- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test --test test_suite test_inplace_rename`
  - Expected: FAIL
- [ ] **Step 3: Implement `AssetCategory` and 2-level formatting/parsing in `src/types.rs`, `src/organizer/inplace.rs`, and `src/organizer/reconcile.rs`**
  - Define `AssetCategory` with `Display` and `FromStr`.
  - Update `format_inplace_filename` with singular `<Category>` token.
  - Implement category registry lookup in `reconcile.rs` (`Bird`, `Mammalia`, `Insecta`, `Scenery`, `Portrait`, `Abstract`).
- [ ] **Step 4: Run test to verify it passes**
  - Run: `cargo test --test test_suite test_inplace_rename test_reconcile_moves`
  - Expected: PASS
- [ ] **Step 5: Commit**
  - `git add src/types.rs src/organizer/inplace.rs src/organizer/reconcile.rs tests/test_suite/`
  - `git commit -m "feat(naming): implement singular 2-level in-place nomenclature and reconciler"`

---

### Task 4: SQLite Schema Migration & 512-dim Embedding Persistence

**Files:**
- Modify: `src/db.rs`
- Modify: `src/main.rs`
- Test: `tests/test_suite/test_db.rs`

**Interfaces:**
- Produces: `bundle_decisions.category` (`TEXT NOT NULL DEFAULT 'Bird'`)
- Produces: `bundle_decisions.visual_embedding` (`BLOB`, exactly 2048 bytes)
- Produces: `insert_bundle_decision(conn, decision, category, embedding)`
- Produces: `query_decisions(conn) -> Vec<(BundleDecision, AssetCategory, Option<Vec<f32>>)>`

- [ ] **Step 1: Write failing Rust tests in `tests/test_suite/test_db.rs`**
  - Test table migration adds `category` and `visual_embedding` columns idempotently.
  - Test inserting and querying asset with 512-dim `f32` embedding vector.
  - Test legacy database schema automatically upgrades without data loss.
- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test --test test_suite test_db`
  - Expected: FAIL
- [ ] **Step 3: Implement SQLite migration and embedding serialization in `src/db.rs` and `src/main.rs`**
  - Add migration `ALTER TABLE bundle_decisions ADD COLUMN category TEXT NOT NULL DEFAULT 'Bird';`
  - Add migration `ALTER TABLE bundle_decisions ADD COLUMN visual_embedding BLOB;`
  - Serialize `Vec<f32>` to little-endian bytes for storage, deserialize on query.
- [ ] **Step 4: Run test to verify it passes**
  - Run: `cargo test --test test_suite test_db`
  - Expected: PASS
- [ ] **Step 5: Commit**
  - `git add src/db.rs src/main.rs tests/test_suite/test_db.rs`
  - `git commit -m "feat(db): add category column and 512-dim visual embedding BLOB persistence"`

---

### Task 5: Generic `TaxonClassifier` Trait & Extensibility Guardrails

**Files:**
- Modify: `src/classifier.rs`
- Modify: `src/burst.rs`
- Create: `tests/test_suite/test_extensible_taxa_guardrails.rs`

**Interfaces:**
- Produces: `pub trait TaxonClassifier: Send + Sync` (generic over biological and photographic domains)
- Produces: `burst_consensus_multi_taxa(items, confidence_mode, min_confidence) -> Vec<BundleDecision>`

- [ ] **Step 1: Write failing architectural guardrail test in `tests/test_suite/test_extensible_taxa_guardrails.rs`**
  - Assert that mock classifications can be performed on non-avian domains (`Mammalia`, `Insecta`, `Scenery`, `Portrait`).
  - Assert that burst clustering operates on arbitrary category items without panicking.
  - Assert that embeddings are preserved across burst smoothing.
- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test --test test_suite test_extensible_taxa_guardrails`
  - Expected: FAIL
- [ ] **Step 3: Refactor `SpeciesClassifier` to `TaxonClassifier` and adapt `src/burst.rs`**
  - Generalize classification outputs to carry `AssetCategory`.
  - Maintain burst consensus within the same category.
  - Route non-confident items to `RoutingTier::NoBirdDetected` (alias `NoSubjectDetected`).
- [ ] **Step 4: Run test to verify it passes**
  - Run: `cargo test --test test_suite test_extensible_taxa_guardrails`
  - Expected: PASS
- [ ] **Step 5: Commit**
  - `git add src/classifier.rs src/burst.rs tests/test_suite/test_extensible_taxa_guardrails.rs`
  - `git commit -m "refactor(classifier): generalize TaxonClassifier trait with extensibility tests"`

---

### Task 6: Multi-Taxa Hierarchy Mode Directory Organization & CLI Corrections

**Files:**
- Modify: `src/organizer/mod.rs`
- Modify: `src/organizer/correct.rs`
- Test: `tests/test_suite/test_spec_conformance.rs`
- Test: `tests/test_suite/test_cli_correct.rs`

**Interfaces:**
- Produces: `<output_root>/<Category>/<Subject>/<OriginalFileName>`
- Produces: `avicull correct --file <FILE> --category <CAT> --species <SPEC>`

- [ ] **Step 1: Write failing Rust tests in `tests/test_suite/test_spec_conformance.rs` and `test_cli_correct.rs`**
  - Test organizing an asset with category `Mammalia` routes hard link to `birds_hierarchy/Mammalia/Malabar_Giant_Squirrel/<file>`.
  - Test organizing an asset with category `Bird` routes to `birds_hierarchy/Bird/White_throated_Kingfisher/<file>`.
  - Test `_No_Subject_Detected/` contains unclassified files, with symlink to `_No_Bird_Detected/`.
  - Test `avicull correct` allows overriding category and species simultaneously.
- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test --test test_suite test_spec_conformance test_cli_correct`
  - Expected: FAIL
- [ ] **Step 3: Implement multi-taxa routing and updated correct command in `src/organizer/mod.rs` and `src/organizer/correct.rs`**
  - Construct category directory branches dynamically.
  - Update `build_output_destination_path` to include singular `category`.
  - Extend `correct` subcommand parser in `src/cli.rs` and `src/organizer/correct.rs` with `--category`.
- [ ] **Step 4: Run test to verify it passes**
  - Run: `cargo test --test test_suite test_spec_conformance test_cli_correct`
  - Expected: PASS
- [ ] **Step 5: Commit**
  - `git add src/organizer/mod.rs src/organizer/correct.rs src/cli.rs tests/test_suite/`
  - `git commit -m "feat(hierarchy): route hard links into category directories and update correct CLI"`

---

### Task 7: Interactive HTML Catalog Evolution (`bird_catalog.html` / `catalog.html`)

**Files:**
- Modify: `src/catalog/mod.rs`
- Modify: `src/catalog/template.rs`
- Test: `tests/test_suite/test_catalog.rs`
- Test: `tests/e2e/catalog_taxa_categories.spec.ts`

**Interfaces:**
- Consumes: `CatalogItem` with `category: AssetCategory`
- Produces: `bird_catalog.html` with Category Pills (`All`, `Bird`, `Mammalia`, `Insecta`, `Scenery`), card tags, and search.

- [ ] **Step 1: Write failing Rust unit test in `tests/test_suite/test_catalog.rs`**
  - Test `CatalogItem` serialization includes `category: "Mammalia"`.
  - Test HTML template contains category filter pills.
- [ ] **Step 2: Write failing Playwright spec in `tests/e2e/catalog_taxa_categories.spec.ts`**
  - Test clicking `Mammalia` pill isolates mammal cards.
  - Test typing `"squirrel"` in search bar filters grid.
  - Test correction modal produces `avicull correct --category Mammalia --species ...`.
- [ ] **Step 3: Implement catalog category UI in `src/catalog/mod.rs` and `src/catalog/template.rs`**
  - Add `category` to `CatalogItem`.
  - Inject category pill markup and JS filtering logic into `template.rs`.
  - Emit both `bird_catalog.html` and symlink/alias `catalog.html`.
- [ ] **Step 4: Run Rust tests to verify they pass**
  - Run: `cargo test --test test_suite test_catalog`
  - Expected: PASS
- [ ] **Step 5: Run Playwright test suite**
  - Run: `cd tests/e2e && npx playwright test catalog_taxa_categories.spec.ts`
  - Expected: PASS
- [ ] **Step 6: Commit**
  - `git add src/catalog/ tests/test_suite/test_catalog.rs tests/e2e/`
  - `git commit -m "feat(catalog): add category filter pills, card badges, and unified search to HTML catalog"`

---

### Task 8: Unified Verification Pipeline & Mock Directory End-to-End Test

**Files:**
- Modify: `tests/test_mock_directory_structure_e2e.rs`
- Run: `scripts/verify.sh`

- [ ] **Step 1: Update mock directory end-to-end test in `tests/test_mock_directory_structure_e2e.rs`**
  - Include sample mock non-avian assets (butterfly, squirrel, scenery) alongside bird assets.
  - Verify APFS hardlink integrity under `Bird/`, `Mammalia/`, `Insecta/`, and `Scenery/`.
  - Verify `manifest.sqlite` contains non-empty 512-dim visual embeddings.
- [ ] **Step 2: Run mock directory test**
  - Run: `cargo test --test test_mock_directory_structure_e2e`
  - Expected: PASS
- [ ] **Step 3: Run complete 7-stage verification script**
  - Run: `./scripts/verify.sh`
  - Expected: PASS on all 7 stages:
    1. `cargo fmt --check`
    2. `cargo check --all-targets`
    3. `cargo clippy --all-targets --all-features -- -D warnings`
    4. `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`
    5. Python verification suite (`verify_python.sh`: ruff, mypy, pytest $\ge 80\%$)
    6. `cargo deny check`
    7. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
- [ ] **Step 4: Commit**
  - `git add tests/test_mock_directory_structure_e2e.rs`
  - `git commit -m "test: verify multi-taxa classification and semantic embeddings end-to-end"`
