# Implementation Plan: Photographic Genres Expansion (Option A)

This plan expands `avicull`'s category routing and taxonomy system to support natural and photographic genres (**Botanical**, **Architecture**, **Portrait**, **Abstract**) alongside existing wildlife categories (**Bird**, **Mammalia**, **Insecta**, **Reptilia**, **Amphibia**), **Scenery** (Landscape), and **NoSubjectDetected**.

---

## 1. Architectural Principles & Performance Invariant

1. **Zero Inference Time & Memory Overhead**:
   - `BioCLIP`/`SigLIP` generates a 512-dimensional embedding vector $z \in \mathbb{R}^{512}$ for every processed image or burst.
   - Stage 1 category centroid projection involves taking the dot product $s_k = z \cdot c_k$ against category centroids $c_k \in \mathbb{R}^{512}$.
   - Expanding from 6 centroids to 10 centroids increases vector operations by 4 dot products ($4 \times 512$ floating-point multiplications and additions), which requires $< 0.01 \text{ ms}$ on CPU/GPU and consumes 0 additional RAM during batch inference.
2. **Backward Compatibility & Invariant Integrity**:
   - `Scenery` remains fully preserved as the canonical scenic/landscape category.
   - In-place reorganizations and migrations recognize both old and new category prefixes.
   - Strict source immutability, zero-byte APFS hard links, and non-destructive operations are preserved.

---

## 2. Changes Overview

### A. Domain Model (`src/types.rs`)
- Expand `AssetCategory` enum:
  - Add `Botanical` (`"Botanical"`)
  - Add `Architecture` (`"Architecture"`)
  - Add `Portrait` (`"Portrait"`)
  - Add `Abstract` (`"Abstract"`)
- Update `AssetCategory::as_str()`, `FromStr`, `is_wildlife()`, `folder_prefix()`, `all()`.
- Update `AssetCategory::from_curated_keywords()` with standard representative nouns:
  - Botanical: *flower, tree, plant, flora, leaf, blossom, cactus, orchid, fungus, mushroom, lichen, moss, foliage*
  - Architecture: *building, tower, bridge, temple, church, mosque, monument, palace, castle, ruins, facade, cathedral, skyscraper*
  - Portrait: *portrait, face, person, people, headshot, smile, gaze*
  - Abstract: *abstract, pattern, texture, macro texture, reflection, minimalist, geometry*

### B. Catalog Statistics & UI (`src/catalog/mod.rs`, `src/catalog/template.rs`)
- Add category count fields in `CatalogStats`:
  - `botanical_count`, `architecture_count`, `portrait_count`, `abstract_count`
- Add UI filter pills and category badging in the HTML catalog template.
- Add category detection in JavaScript `resolveTargetCategory()` for catalog correction modal.

### C. Organizer & In-Place Processing (`src/organizer/mod.rs`, `src/organizer/inplace.rs`)
- Recognize new category subdirectories and prefix paths during in-place rename, verify, and migration runs.

### D. Python Inference & Embeddings Generator (`scripts/build_taxonomy_embeddings.py`, `scripts/infer.py`)
- Update `CANONICAL_CATEGORIES` to include `"Botanical"`, `"Architecture"`, `"Portrait"`, `"Abstract"`.
- Add curated exemplar seeds in `build_taxonomy_embeddings.py` for each new genre.
- Add test coverage in `tests_python/test_infer.py` and `tests/test_build_gazetteer.py`.

---

## 3. Step-by-Step Implementation & Verification (TDD)

### Step 1: Rust Unit & Integration Tests (RED)
- Add tests in `tests/test_suite/` verifying:
  - `AssetCategory` parsing, string roundtrip, folder prefix, and curated keyword resolution for new genres.
  - Catalog stats count calculations for the new categories.
  - Path parsing and category detection in `src/organizer/mod.rs`.
- Run `cargo test --test test_suite` to observe failures.

### Step 2: Rust Domain & Catalog Implementation (GREEN)
- Implement `AssetCategory` variants in `src/types.rs`.
- Update `CatalogStats` and `src/catalog/mod.rs`.
- Update HTML template in `src/catalog/template.rs` with new filter buttons and badges.
- Update `examples/generate_fixture_catalog.rs`.
- Re-run `cargo test --test test_suite`.
- Update snapshot test: `UPDATE_SNAPSHOTS=1 cargo test --test test_suite -- test_catalog_snapshot`.

### Step 3: Python Pipeline & Test Updates (RED -> GREEN)
- Add tests in `tests_python/` asserting new categories exist in `CANONICAL_CATEGORIES` and are resolved properly.
- Update `scripts/build_taxonomy_embeddings.py` and `scripts/infer.py`.
- Run `uv run pytest` and verify $>80\%$ coverage.

### Step 4: Verification Pipeline
- Run full `./scripts/verify.sh` (fmt, check, clippy, tests, python checks, deny, rustdoc).
- Run on real user directory `/Users/shaleensharma/Pictures/leh` to validate end-to-end.
