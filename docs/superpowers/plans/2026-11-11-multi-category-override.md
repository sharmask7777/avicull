# Multi-Category & Photographic Genre Human Overrides Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend human corrective overrides and the interactive catalog modal from bird-only to all biological classes (Mammalia, Reptilia, Amphibia, Insecta) and photographic genres (Scenery, Portrait, Architecture, Abstract, No Subject) with a strongly-typed `Classification` domain model and Option A category pill UI.

**Architecture:** Refactor `BundleDecision` to use a first-class `Classification { category: AssetCategory, entity_slug: Option<String> }` struct that cleanly supports both top-level kingdom/genre filing and granular species without string guessing. In the catalog, serialize a compact `multi_taxa` dataset (~40 KB) alongside eBird `species_list`, and upgrade the correction modal to provide dynamic category pills and scoped autocomplete.

**Tech Stack:** Rust (1.80+), SQLite (`rusqlite`), Vanilla JS / CSS in HTML catalog template, Playwright (TypeScript) for E2E validation.

**Spec:** `docs/superpowers/specs/2026-11-11-multi-category-override-design.md`

## Global Constraints

- Mode Invariant 1: In `--mode in-place` (default), the input archive is strictly read-only; source files are never moved, renamed, or modified.
- Mode Invariant 2: In `--mode in-place-rename`, source files are renamed in-place with `<Category>_<Species>` tokens (or `<Category>` if top-level) with transaction logs in `in_place_rename`.
- Mode Invariant 3: In `--mode hierarchy`, assets are projected via POSIX hard links (`st_nlink >= 2`, 0 bytes disk usage) into `birds_hierarchy/<Category>/<Species>/` (or `birds_hierarchy/<Category>/` if top-level).
- Anti-Goal: Never assume unknown entities or slugs are birds during automated fallback; explicit human overrides specifying `--category Bird` are strictly preserved.
- Compiler & Linter: Zero warnings with `unsafe_code = "deny"`, `clippy::pedantic`, and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- Testing Architecture: All integration tests are consolidated under `tests/test_suite/` exposed in `tests/test_suite/main.rs`.

## Review Focus

1. Empty species input for biological kingdoms (e.g. `--category Mammalia` without `--species`): must route to `birds_hierarchy/Mammalia/` or `<stem>_Mammalia.<ext>` without panicking or creating trailing underscores.
2. Custom freeform species under `Bird` (e.g. `--category Bird --species "Gyrfalcon_x_Peregrine"`): must be preserved as `AssetCategory::Bird` with `is_user_override = true` rather than being downgraded to `NoSubjectDetected`.
3. Non-avian typo prompts: when `--category Mammalia --species Snow_Leopard` is executed via CLI, it must never prompt `Species 'Snow_Leopard' not found in taxonomy. Did you mean 'Snow Bunting'?`.
4. Backward compatibility of `CatalogData`: existing snapshot tests checking `species_list` must remain 100% stable while `multi_taxa` is loaded.
5. Batch correction of mixed categories: batch scripts must correctly emit discrete `--category` flags per item in `apply_corrections.sh`.

---

### Task 1: `Classification` Domain Struct and `BundleDecision` Refactor

**Files:**
- Modify: `src/types.rs:475-520`
- Test: `tests/test_suite/test_classification_types.rs`
- Modify: `tests/test_suite/main.rs`

**Interfaces:**
- Consumes: `AssetCategory` from `src/types.rs`
- Produces:
  ```rust
  pub struct Classification {
      pub category: AssetCategory,
      pub entity_slug: Option<String>,
  }
  impl Classification {
      pub fn top_level(category: AssetCategory) -> Self;
      pub fn granular(category: AssetCategory, entity_slug: impl Into<String>) -> Self;
  }
  impl BundleDecision {
      pub fn category(&self) -> &AssetCategory;
      pub fn entity_slug(&self) -> Option<&str>;
      pub fn assigned_species_slug(&self) -> &str;
      pub fn destination_path_tokens(&self) -> (&str, Option<&str>);
  }
  ```

- [ ] **Step 1: Write the failing test in `tests/test_suite/test_classification_types.rs`**

```rust
use avicull::types::{AssetCategory, BundleDecision, Classification, RoutingTier};

#[test]
fn test_classification_top_level_and_granular() {
    let top_mammal = Classification::top_level(AssetCategory::Mammalia);
    assert_eq!(top_mammal.category, AssetCategory::Mammalia);
    assert_eq!(top_mammal.entity_slug, None);

    let granular_mammal = Classification::granular(AssetCategory::Mammalia, "Snow_Leopard");
    assert_eq!(granular_mammal.category, AssetCategory::Mammalia);
    assert_eq!(granular_mammal.entity_slug.as_deref(), Some("Snow_Leopard"));

    let empty_granular = Classification::granular(AssetCategory::Scenery, "   ");
    assert_eq!(empty_granular.entity_slug, None);
}

#[test]
fn test_bundle_decision_classification_accessors() {
    let dec = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        classification: Classification::granular(AssetCategory::Mammalia, "Snow_Leopard"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    assert_eq!(dec.category(), &AssetCategory::Mammalia);
    assert_eq!(dec.entity_slug(), Some("Snow_Leopard"));
    assert_eq!(dec.assigned_species_slug(), "Snow_Leopard");
    assert_eq!(dec.destination_path_tokens(), ("Mammalia", Some("Snow_Leopard")));

    let top_dec = BundleDecision {
        classification: Classification::top_level(AssetCategory::Scenery),
        ..dec
    };
    assert_eq!(top_dec.category(), &AssetCategory::Scenery);
    assert_eq!(top_dec.entity_slug(), None);
    assert_eq!(top_dec.assigned_species_slug(), "");
    assert_eq!(top_dec.destination_path_tokens(), ("Scenery", None));
}
```

- [ ] **Step 2: Register test in `tests/test_suite/main.rs` and verify it fails**

Run: `cargo test --test test_suite test_classification -v`
Expected: FAIL with `no field or method named classification on BundleDecision`

- [ ] **Step 3: Implement `Classification` and refactor `BundleDecision` in `src/types.rs`**

Add `pub classification: Classification` to `BundleDecision`, implement `Classification::top_level`, `Classification::granular`, and the convenience methods `category()`, `entity_slug()`, `assigned_species_slug()`, and `destination_path_tokens()`. Update existing call sites in `src/` to initialize `classification`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_classification -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/types.rs tests/test_suite/
git commit -m "feat(types): introduce Classification domain struct on BundleDecision"
```

---

### Task 2: Database Manifest Persistence & Querying for `Classification`

**Files:**
- Modify: `src/db.rs`
- Test: `tests/test_suite/test_classification_db.rs`
- Modify: `tests/test_suite/main.rs`

**Interfaces:**
- Consumes: `Classification`, `BundleDecision` from `src/types.rs`
- Produces:
  - `ManifestDb::insert_bundle_decision(decision: &BundleDecision)` saving `decision.category().as_str()` and `decision.entity_slug().unwrap_or("")`
  - `ManifestDb::get_all_bundles()` reconstructing `BundleDecision` with `Classification`

- [ ] **Step 1: Write the failing test in `tests/test_suite/test_classification_db.rs`**

```rust
use avicull::db::ManifestDb;
use avicull::types::{AssetCategory, BundleDecision, Classification, RoutingTier};
use tempfile::tempdir;

#[test]
fn test_persist_and_retrieve_classification() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path)?;

    let dec = BundleDecision {
        bundle_id: "test_b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Mammalia, "Snow_Leopard"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    db.upsert_decision(&dec)?;
    let retrieved = db.get_decision_by_bundle_id("test_b1")?.expect("bundle decision exists");

    assert_eq!(retrieved.category(), &AssetCategory::Mammalia);
    assert_eq!(retrieved.entity_slug(), Some("Snow_Leopard"));
    assert!(retrieved.is_user_override);

    let top_dec = BundleDecision {
        bundle_id: "test_b2".to_string(),
        classification: Classification::top_level(AssetCategory::Scenery),
        ..dec
    };
    db.upsert_decision(&top_dec)?;
    let retrieved_top = db.get_decision_by_bundle_id("test_b2")?.expect("bundle decision exists");

    assert_eq!(retrieved_top.category(), &AssetCategory::Scenery);
    assert_eq!(retrieved_top.entity_slug(), None);

    Ok(())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_persist_and_retrieve_classification -v`
Expected: FAIL on compilation or missing `get_decision_by_bundle_id` / `upsert_decision` classification reconstruction.

- [ ] **Step 3: Update `src/db.rs` mapping**

Update `insert_bundle_decision`, `upsert_decision`, `get_all_bundles`, and query mappers to populate `decision.classification` from the database `category` and `assigned_species_slug` columns, treating empty or whitespace `assigned_species_slug` as `entity_slug: None`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_persist_and_retrieve_classification -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/db.rs tests/test_suite/
git commit -m "feat(db): persist and reconstruct Classification in manifest database"
```

---

### Task 3: CLI `CorrectArgs` & Core Organizer Logic

**Files:**
- Modify: `src/cli.rs:450-465`
- Modify: `src/organizer/correct.rs:230-480`
- Modify: `src/organizer/engine.rs`
- Modify: `src/organizer/inplace.rs`
- Test: `tests/test_suite/test_multi_category_correct.rs`
- Modify: `tests/test_suite/main.rs`

**Interfaces:**
- Consumes: `Classification`, `BundleDecision` from `src/types.rs`, `ManifestDb` from `src/db.rs`
- Produces:
  - `apply_bundle_correction_with_category(..., category: &str, species: &str, ...)`
  - Optional `--species` in CLI `CorrectArgs` (default `""`)

- [ ] **Step 1: Write the failing test in `tests/test_suite/test_multi_category_correct.rs`**

```rust
use avicull::cli::OrganizationMode;
use avicull::db::ManifestDb;
use avicull::organizer::apply_bundle_correction_with_category;
use avicull::types::{AssetCategory, AssetBundle, BundleDecision, Classification, RoutingTier};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_correct_non_avian_granular_and_top_level() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let input_dir = dir.path().join("input");
    fs::create_dir_all(&input_dir)?;
    let img_path = input_dir.join("DSC_0001.JPG");
    fs::write(&img_path, b"fake jpeg payload")?;

    let db_path = input_dir.join(".avicull").join("manifest.sqlite");
    fs::create_dir_all(db_path.parent().unwrap())?;
    let db = ManifestDb::open_or_create(&db_path)?;

    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        primary_image: img_path.clone(),
        sidecars: vec![],
        source_inode: 1234,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0001".to_string(),
        capture_timestamp_ms: None,
        lat: None,
        lon: None,
        location_source: None,
        relative_path: Some(PathBuf::from("DSC_0001.JPG")),
        was_dataless: false,
    };
    let dec = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Bird, "House_Sparrow"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };
    db.upsert_bundle_and_decision(&bundle, &dec)?;

    // Test 1: Override to granular mammal in default in-place mode (source unchanged, manifest updated)
    let res = apply_bundle_correction_with_category(
        &db,
        &input_dir,
        OrganizationMode::InPlace,
        "DSC_0001",
        "Mammalia",
        "Snow_Leopard",
        Some(&input_dir),
        false,
    )?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.category(), &AssetCategory::Mammalia);
    assert_eq!(res[0].0.entity_slug(), Some("Snow_Leopard"));
    assert!(res[0].0.is_user_override);

    // Verify source file on disk is strictly untouched
    assert!(img_path.exists());

    // Test 2: Override to top-level Scenery with blank species
    let res_top = apply_bundle_correction_with_category(
        &db,
        &input_dir,
        OrganizationMode::InPlace,
        "DSC_0001",
        "Scenery",
        "",
        Some(&input_dir),
        false,
    )?;
    assert_eq!(res_top.0.category(), &AssetCategory::Scenery);
    assert_eq!(res_top.0.entity_slug(), None);

    Ok(())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_correct_non_avian -v`
Expected: FAIL due to string normalization or bird typo suggestion fallback.

- [ ] **Step 3: Update `src/cli.rs` and `src/organizer/correct.rs`**

- Set `#[arg(short, long, default_value = "")] pub species: String` on `CorrectArgs`.
- In `apply_bundle_correction_with_category`:
  - When category is non-avian, suppress `find_species_typo_suggestion` against birds.
  - Parse `new_species`: if empty or whitespace, assign `entity_slug: None`.
  - When `--category "Bird"` is explicitly supplied with a custom slug (e.g. `"Gyrfalcon_x_Peregrine"`), preserve `AssetCategory::Bird` with `is_user_override = true`.
  - In `hierarchy` mode: link to `birds_hierarchy/<category>/<entity>/` (or `birds_hierarchy/<category>/` if None).
  - In `in-place-rename` mode: format stem as `<stem>_<category>_<entity>` (or `<stem>_<category>` if None).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_correct_non_avian -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/organizer/ tests/test_suite/
git commit -m "feat(correct): support multi-category and top-level overrides across modes"
```

---

### Task 4: Catalog Data Model & Multi-Taxa Serialization

**Files:**
- Modify: `src/catalog/mod.rs`
- Test: `tests/test_suite/test_catalog_multi_taxa.rs`
- Modify: `tests/test_suite/main.rs`

**Interfaces:**
- Consumes: `curated_multi_taxa.json` from `data/`, `CatalogData` from `src/catalog/mod.rs`
- Produces:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct CuratedTaxonEntry {
      pub taxon_id: String,
      pub category: String,
      pub common_name: String,
      pub scientific_name: Option<String>,
      pub aliases: Vec<String>,
  }
  ```
  `CatalogData.multi_taxa: Vec<CuratedTaxonEntry>`

- [ ] **Step 1: Write the failing test in `tests/test_suite/test_catalog_multi_taxa.rs`**

```rust
use avicull::catalog::generate_catalog_data;
use avicull::cli::OrganizationMode;
use avicull::types::{AssetBundle, BundleDecision, Classification, AssetCategory, RoutingTier};
use std::path::PathBuf;

#[test]
fn test_catalog_data_embeds_multi_taxa() {
    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        primary_image: PathBuf::from("DSC_0001.JPG"),
        sidecars: vec![],
        source_inode: 1,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0001".to_string(),
        capture_timestamp_ms: None,
        lat: None,
        lon: None,
        location_source: None,
        relative_path: Some(PathBuf::from("DSC_0001.JPG")),
        was_dataless: false,
    };
    let dec = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Mammalia, "Snow_Leopard"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: true,
    };

    let data = generate_catalog_data(&[(bundle, dec)], OrganizationMode::InPlace, &PathBuf::from("."));

    // Invariant: species_list for birds remains populated (snapshot backward compatibility)
    assert!(!data.species_list.is_empty());

    // Invariant: multi_taxa contains curated non-avian kingdoms
    assert!(!data.multi_taxa.is_empty());
    assert!(data.multi_taxa.iter().any(|t| t.category == "Mammalia" && t.common_name == "Snow Leopard"));
    assert!(data.multi_taxa.iter().any(|t| t.category == "Reptilia"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_suite test_catalog_data_embeds_multi_taxa -v`
Expected: FAIL with `no field multi_taxa on CatalogData`

- [ ] **Step 3: Implement `CuratedTaxonEntry` and load multi-taxa in `src/catalog/mod.rs`**

Add `pub multi_taxa: Vec<CuratedTaxonEntry>` to `CatalogData`. In `generate_catalog_data`, parse `CURATED_MULTI_TAXA_JSON` into `data.multi_taxa`. Ensure `data.photos` items serialize `category: item.category.as_str()` and `species: item.entity_slug().unwrap_or("")`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_suite test_catalog_data_embeds_multi_taxa -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/catalog/ tests/test_suite/
git commit -m "feat(catalog): embed multi-taxa taxonomy entries into catalog payload"
```

---

### Task 5: Interactive Modal HTML & Dynamic Autocomplete JavaScript

**Files:**
- Modify: `src/catalog/template.rs`
- Test: `tests/e2e/multi_category_override.spec.ts`

**Interfaces:**
- Consumes: `CATALOG_DATA.multi_taxa`, `CATALOG_DATA.species_list`
- Produces:
  - Option A Category pills in `#correctionModal`
  - Dynamic category-scoped autocomplete in `handleSpeciesAutocomplete()`
  - Real-time command preview and `apply_corrections.sh` generator with `--category`

- [ ] **Step 1: Write Playwright E2E test in `tests/e2e/multi_category_override.spec.ts`**

```typescript
import { test, expect } from '@playwright/test';
import * as path from 'path';

test('multi-category correction modal flow', async ({ page }) => {
  const fixturePath = 'file://' + path.resolve(__dirname, 'fixtures/mock_catalog.html');
  await page.goto(fixturePath);

  // Open correction modal for first photo card
  const correctBtn = page.locator('.btn-correct').first();
  await correctBtn.click();

  const modal = page.locator('#correctionModal');
  await expect(modal).toBeVisible();

  // Verify category pills are rendered
  const mammaliaPill = modal.locator('.cat-pill', { hasText: 'Mammalia' });
  await expect(mammaliaPill).toBeVisible();

  // Click Mammalia pill
  await mammaliaPill.click();
  await expect(mammaliaPill).toHaveClass(/active/);

  // Type in autocomplete and verify mammal suggestions appear
  const searchInput = modal.locator('#speciesSearchInput');
  await searchInput.fill('Snow');

  const sugItem = modal.locator('.suggestion-item', { hasText: 'Snow Leopard' });
  await expect(sugItem).toBeVisible();
  await sugItem.click();

  // Verify generated command in command box
  const commandBox = modal.locator('#commandOutput');
  await expect(commandBox).toContainText('--category "Mammalia"');
  await expect(commandBox).toContainText('--species "Snow_Leopard"');
});
```

- [ ] **Step 2: Run Playwright test to verify it fails**

Run: `cd tests/e2e && npx playwright test multi_category_override.spec.ts`
Expected: FAIL with `.cat-pill not found`

- [ ] **Step 3: Update `src/catalog/template.rs` HTML and JavaScript**

1. In the HTML of `#correctionModal`:
   - Replace bird-only title with `Correct Classification`.
   - Add category pill group (`Bird`, `Mammalia`, `Reptilia`, `Amphibia`, `Insecta`, `Scenery`, `Portrait`, `Architecture`, `Abstract`, `No Subject`).
2. In client JavaScript:
   - Index `CATALOG_DATA.multi_taxa` by category.
   - Implement `selectCategory(cat)`: sets `activeCategory`, switches placeholder, and clears/filters suggestions.
   - For `No Subject`: disables species input, sets slug to `_No_Subject_Detected`.
   - Update `buildMoveCommandsForItem`: pass `--category "${cat}"` and `--species "${slug}"` (omitting `--species` if blank).
   - Update `apply_corrections.sh` builder to include `--category`.

- [ ] **Step 4: Run Playwright test to verify it passes**

Run: `cd tests/e2e && npx playwright test multi_category_override.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/catalog/template.rs tests/e2e/
git commit -m "feat(catalog): implement Option A multi-category pill modal and dynamic autocomplete"
```

---

### Task 6: Unified Pipeline Verification & Documentation

**Files:**
- Modify: `tests/fixtures/snapshots/` (if HTML snapshot updated)
- Modify: `docs/solutions/` (architecture pattern note)
- Test: `./scripts/verify.sh`

- [ ] **Step 1: Run full Rust test suite and snapshot check**

Run: `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`
Expected: All tests pass.

- [ ] **Step 2: Run Clippy lints**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 3: Run Rustdoc documentation gate**

Run: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
Expected: 0 warnings.

- [ ] **Step 4: Run canonical verification pipeline**

Run: `./scripts/verify.sh`
Expected: All 7 stages pass cleanly.

- [ ] **Step 5: Commit**

```bash
git add .
git commit -m "chore: verify full pipeline and update documentation for multi-category overrides"
```
