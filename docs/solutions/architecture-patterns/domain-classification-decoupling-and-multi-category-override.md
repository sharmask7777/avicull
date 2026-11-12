---
title: "Decoupling Domain Classification into Strongly Typed Category and Granular Entity Target"
date: "2026-11-12"
category: "architecture-patterns"
module: "types"
problem_type: "architecture_pattern"
component: "data_model"
severity: "medium"
applies_when:
  - "Extending human curation, CLI override, or ML classification to support multi-taxa wildlife (Mammalia, Insecta, Reptilia, Amphibia) alongside photographic genres (Scenery, Portrait, Architecture)"
  - "Supporting top-level kingdom or genre filing when granular species names are blank"
  - "Preserving explicit user classifications for novel bird hybrids while preventing non-avian or botanical entities from polluting avian taxonomy"
tags:
  - "classification-struct"
  - "domain-modeling"
  - "multi-category-override"
  - "botanical-guardrails"
  - "sqlite-manifest"
---

# Decoupling Domain Classification into Strongly Typed Category and Granular Entity Target

## Problem
Prior to multi-category human curation, `BundleDecision` recorded classifications using a single `assigned_species_slug: String` field, relying on ad-hoc string parsing (`AssetCategory::from_slug(&slug)`) to infer categories after the fact.

This ad-hoc string guessing produced three severe architectural and behavioral limitations:
1. **Inability to Support Top-Level Kingdom or Genre Filing**: When classifying a photo into a broad genre like `Scenery` or `Portrait` without a granular sub-folder, `assigned_species_slug` was either forced to a sentinel, left empty, or duplicated as `"Scenery"`. This caused in-place rename formatters to emit redundant double underscores (e.g. `DSC_0001__20261102.JPG`) and hierarchy projection to construct redundant nested subfolders (`birds_hierarchy/Scenery/Scenery/`).
2. **False Demotion of Novel Bird Hybrids**: If a photographer curated an asset with an explicit `--category Bird` and a custom species or hybrid name not in the eBird/Clements taxonomy (e.g. `Mallard_x_Northern_Pintail` or `Custom_Avian_Hybrid`), string inference defaulted to `AssetCategory::NoSubjectDetected`, forcibly demoting the confirmed bird to an unclassified sentinel.
3. **Desynchronized Database Manifest Loading**: Manifest query functions (`query_decisions` and `get_all_bundle_decisions`) implemented divergent row deserialization heuristics when reading `category` and `assigned_species_slug` from SQLite, creating subtle consistency bugs across pipeline passes.

## Solution

### 1. Strongly Typed `Classification` Domain Struct
In `src/types.rs`, introduced a first-class `Classification` struct that decouples the broad taxonomic/photographic category from the optional granular species or entity slug:

```rust
// src/types.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    /// Domain category: Bird, Mammalia, Reptilia, Amphibia, Insecta,
    /// Scenery, Portrait, Architecture, Abstract, NoSubjectDetected
    pub category: AssetCategory,

    /// Granular entity or species slug, or None for top-level kingdom or genre filing.
    /// e.g. Some("Snow_Leopard"), Some("Mountain"), or None for top-level "Mammalia" / "Scenery".
    pub entity_slug: Option<String>,
}
```

Replaced `assigned_species_slug: String` on `BundleDecision` with `pub classification: Classification`, implementing backward-compatible deserialization and accessor methods (`.category()`, `.entity_slug()`, `.assigned_species_slug()`, `.destination_path_tokens()`).

### 2. Guardrails Differentiating Botanical Motifs from Novel Avian Taxa
In `src/organizer/correct.rs`, the pipeline previously demoted any slug not in `KNOWN_BIRD_SET` to `NoSubjectDetected` to prevent botanical subjects like `Sacred_Lotus` from being filed under `Bird`.

To preserve explicit user intent for custom avian hybrids while maintaining botanical protection, introduced `is_known_botanical_slug` in `src/types.rs`:
- If an entity matches `KNOWN_BOTANICAL_SET` (e.g. `banyan_tree`, `sacred_lotus`, `rhododendron`), it is strictly demoted to `AssetCategory::NoSubjectDetected`.
- If an entity is not botanical, not in `CURATED_MULTI_TAXA_MAP`, and the user explicitly passed `--category Bird`, their explicit category selection is honored as `AssetCategory::Bird`.

### 3. Centralized SQLite Deserialization
Added `Classification::from_db_fields(category: AssetCategory, assigned_species_slug: &str) -> Classification` in `src/types.rs` and replaced disparate row-mapping code in `src/db.rs` (`get_all_bundle_decisions` and `query_decisions`).

If the stored `assigned_species_slug` matches the category name or starts with an underscore sentinel, `entity_slug` resolves cleanly to `None`, ensuring top-level genres never create nested directories.

### 4. Resilient Path Formatting Across Modes
- **`--mode in-place-rename`**: `format_inplace_filename` skips the subject token when `entity_slug` is `None`, generating `<Stem>_<Category>[_<Date>][_<Location>].<ext>` (e.g. `DSC_0001_Scenery_20261102_Leh.JPG`) without double underscores.
- **`--mode hierarchy`**: `build_output_destination_path` routes top-level genres to `birds_hierarchy/<Category>/` rather than `birds_hierarchy/<Category>/<Category>/`.

## Why This Works
- Strong typing eliminates fragile string guessing and prefix sniffing across all 55+ downstream decision consumers.
- Explicit `--category Bird` inputs are honored for novel hybrids without compromising guardrails against deprecated botanical fallbacks.
- Single-point database mapping guarantees identical domain representations regardless of which manifest query method was invoked.

## Prevention
- Regression tests in `tests/test_suite/test_multi_category_correct.rs` test custom bird hybrids without taxonomy suggestions (`test_correct_custom_avian_hybrid_without_suggestion_preserves_bird`) and botanical demotion (`test_correct_botanical_species_demotes_to_no_subject_detected`).
- Unit tests in `tests/test_suite/test_classification_db.rs` verify database persistence and roundtrip fidelity.
- Interactive catalog Playwright tests in `tests/e2e/multi_category_override.spec.ts` exercise modal category switching and command generation.

## Related PRs & Code
- Solved and implemented in PR #61
- Core implementations in `src/types.rs`, `src/db.rs`, `src/organizer/correct.rs`, `src/organizer/mod.rs`, and `src/catalog/template.rs`
