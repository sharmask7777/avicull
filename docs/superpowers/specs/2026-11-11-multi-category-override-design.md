# Multi-Category & Photographic Genre Human Overrides Design

**Status:** Draft / Under Review  
**Date:** 2026-11-11  
**Target:** `avicull` (HTML Catalog View, CLI `correct`, ManifestStore, and Storage Engines)

---

## 1. Intent & The Why

`avicull` currently provides an in-browser mislabel correction modal and CLI interface (`avicull correct`) specifically tailored for avian species. When reviewing an archive, photographers frequently capture non-avian wildlife (mammals, reptiles, amphibians, insects) or non-biological photographic genres (scenery, portraits, architecture, abstract patterns). 

Today, while the backend classifier separates assets into categories (`AssetCategory`) and stores multi-taxa in `curated_multi_taxa.json`, the interactive catalog correction modal only allows searching Cornell eBird species and defaults all corrections to `AssetCategory::Bird`.

The goal of this project is to provide the **exact same frictionless correction experience** across all biological categories and photographic genres:
1. Allow photographers to change a photo's (or batch of photos') category with one click.
2. Provide responsive autocomplete for granular species within biological kingdoms (Mammalia, Reptilia, Amphibia, Insecta, Bird) and sub-folders/genres for photographic styles (Scenery, Portrait, Architecture, Abstract).
3. Generate valid, mode-aware commands for `avicull correct` and shell scripts (`apply_corrections.sh`) that record human overrides permanently in `.avicull/manifest.sqlite`.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Unified Modal Flow (Option A)**: Category selector pills (`Bird`, `Mammalia`, `Reptilia`, `Amphibia`, `Insecta`, `Scenery`, `Portrait`, `Architecture`, `Abstract`, `No Subject`) displayed at the top of the modal, pre-selected to the photo's current category.
- **Dynamic Scoped Autocomplete**: Switching category pills dynamically switches the autocomplete index to that domain (e.g. mammals, reptiles, or scenery sub-genres).
- **Freeform Entry**: Photographers can type any custom species name or genre sub-folder and submit it, ensuring they are never blocked if an animal or genre isn't in the curated dataset. When a human explicitly selects a category (including `Bird`), their explicit categorization is honored and recorded in the manifest as an authoritative human override.
- **Mode-Aware Invariant Handling**:
  - In `--mode hierarchy`: Relinks files into `birds_hierarchy/<Category>/<Species_or_Subfolder>/` (or `birds_hierarchy/<Category>/` if species/subfolder is left blank).
  - In `--mode in-place-rename`: Renames source files in place with `<Category>_<Species_or_Subfolder>` slug tokens (or `<Category>` if blank, matching `parse_inplace_filename` canonical parsing).
  - In `--mode in-place` (Default): Leaves source files strictly read-only and records the override transactionally in SQLite with `is_user_override = 1`.
- **Batch Operations**: Support multi-select batch corrections, batch script queuing, clipboard copying, and `apply_corrections.sh` downloads for any category.
- **Single-Click False Positive Sentinel**: Retain one-click `Mark as No Subject (False Positive)` button that maps to `_No_Subject_Detected` (or `_No_Bird_Detected`).

### Non-Goals
- Changing neural inference architectures or downloading new external machine learning weights in this feature (this feature concerns the human correction, catalog UI, and manifest storage layer).
- Requiring an active backend server: the HTML catalog must remain 100% standalone and offline-capable without a running node/python daemon.

### Anti-Goals
- Never assume an unknown slug or automated inference without an explicit category is a bird (the negative invariant applies to automated fallback; explicit human overrides specifying `--category Bird` are strictly preserved).
- Never violate source immutability in default `--mode in-place`.
- Never wipe or discard previously submitted human corrections during incremental `organize` passes.

---

## 3. User Experience & Modal Interaction (Option A)

### Modal Layout
When a reviewer clicks **"Correct Species"** on a photo card (or presses keyboard shortcut `c`), or selects multiple cards and clicks the batch correction button:

1. **Photo Summary**: Displays thumbnail, current species/category, and relative file path.
2. **Category Selector Pills**:
   - `[ Bird ]` `[ Mammalia ]` `[ Reptilia ]` `[ Amphibia ]` `[ Insecta ]` `[ Scenery ]` `[ Portrait ]` `[ Architecture ]` `[ Abstract ]` `[ No Subject ]`
   - Active pill highlighted with clear contrast and checkmark indicator.
3. **Granular Subject / Species Autocomplete**:
   - **For Biological Kingdoms** (`Bird`, `Mammalia`, `Reptilia`, `Amphibia`, `Insecta`):
     - Placeholder: `"Start typing <category> name (e.g. Snow Leopard)..."`
     - Autocomplete searches common names, scientific names, and aliases from embedded taxonomy.
     - Freeform input is valid even if no suggestion matches.
   - **For Photographic Genres** (`Scenery`, `Portrait`, `Architecture`, `Abstract`):
     - Placeholder: `"Enter sub-folder or tag (e.g. Mountain, Coast) or leave blank for top-level..."`
     - Autocomplete suggests common curated motifs (e.g. `Mountain`, `Sunset`, `Street`, `Candid`).
     - If left blank or genre selected directly, routes to top-level folder in hierarchy mode (`Scenery/`).
   - **For Sentinel** (`No Subject`):
     - Immediately disables/hides species input and sets slug to `_No_Subject_Detected`.
4. **Command Preview & Script Actions**:
   - Displays real-time preview of the generated command:
     ```bash
     avicull correct --category "Mammalia" --species "Snow_Leopard" --file "DSC_1042.JPG"
     ```
   - **"Copy Command"**: Copies single-line command to system clipboard.
   - **"Add to Batch Script"**: Adds correction to internal queue and updates `apply_corrections.sh` counter.

---

## 4. Architectural & Technical Changes

### A. Strongly-Typed Domain Classification Refactor (`src/types.rs`)
Currently, `BundleDecision` lacks an explicit `category` field and relies on string reverse-engineering (`AssetCategory::from_slug(&decision.assigned_species_slug)`), with empty strings or sentinel strings overloaded for missing species.

We introduce a strongly-typed domain struct:
```rust
/// Strongly-typed classification target encompassing broad category and optional granular entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    /// Domain category: Bird, Mammalia, Reptilia, Amphibia, Insecta,
    /// Scenery, Portrait, Architecture, Abstract, NoSubjectDetected
    pub category: AssetCategory,

    /// Granular entity or species slug, or `None` for top-level kingdom or genre filing.
    /// e.g. `Some("Snow_Leopard")`, `Some("Mountain")`, or `None` for top-level "Mammalia" / "Scenery".
    pub entity_slug: Option<String>,
}

impl Classification {
    /// Constructs a top-level category classification without a granular species/entity.
    pub fn top_level(category: AssetCategory) -> Self {
        Self { category, entity_slug: None }
    }

    /// Constructs a granular species or entity classification.
    pub fn granular(category: AssetCategory, entity_slug: impl Into<String>) -> Self {
        let slug = entity_slug.into();
        let entity_slug = if slug.trim().is_empty() { None } else { Some(slug) };
        Self { category, entity_slug }
    }
}
```

And update `BundleDecision`:
```rust
pub struct BundleDecision {
    pub bundle_id: String,
    pub burst_id: Option<String>,
    pub detection_confidence: f32,
    pub bbox_xyxy_norm: Option<[f32; 4]>,
    pub top_predictions: Vec<SpeciesPrediction>,
    pub frame_confidence: f32,
    pub final_confidence: f32,

    /// Strongly-typed classification target.
    pub classification: Classification,

    pub routing_tier: RoutingTier,
    pub is_user_override: bool,
}

impl BundleDecision {
    /// Returns the primary domain category.
    #[must_use]
    pub fn category(&self) -> &AssetCategory {
        &self.classification.category
    }

    /// Returns the optional granular species or entity slug.
    #[must_use]
    pub fn entity_slug(&self) -> Option<&str> {
        self.classification.entity_slug.as_deref()
    }

    /// Backwards-compatible slug accessor for logging and legacy formatters.
    #[must_use]
    pub fn assigned_species_slug(&self) -> &str {
        self.classification.entity_slug.as_deref().unwrap_or("")
    }

    /// Returns destination path tokens: `(category, optional_subfolder)`.
    #[must_use]
    pub fn destination_path_tokens(&self) -> (&str, Option<&str>) {
        (self.category().as_str(), self.entity_slug())
    }
}
```

### B. Catalog Data Serialization (`src/catalog/mod.rs`)
- **Backward Compatibility & Compactness**:
  - Retain `pub species_list: Vec<String>` on `CatalogData` containing all eBird common names. This preserves zero-bloat JSON serialization (~200 KB) and 100% backward compatibility with existing tests, snapshots, and JS helpers.
  - Add `pub multi_taxa: Vec<CuratedTaxonEntry>` on `CatalogData` containing non-avian entries from `data/curated_multi_taxa.json` (~40 KB total), serialized as:
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CuratedTaxonEntry {
        pub taxon_id: String,
        pub category: String,
        pub common_name: String,
        pub scientific_name: Option<String>,
        #[serde(default)]
        pub aliases: Vec<String>,
    }
    ```
- In `CatalogData::from_bundles`:
  - `species_list` continues to provide fast lookup for `Bird`.
  - `multi_taxa` provides structured autocomplete data for `Mammalia`, `Reptilia`, `Amphibia`, `Insecta`, `Scenery`, `Portrait`, `Architecture`, and `Abstract`, including aliases.

### C. Catalog HTML & Client JavaScript (`src/catalog/template.rs`)
- **State Management**:
  - `activeCategory`: string tracking the currently selected category in the modal (defaults to `item.category`).
  - `categoryTaxonomies`: client-side index grouping species/motifs by category.
- **Dynamic Autocomplete Function**:
  - `handleCategoryPillClick(category)`: Updates active pill, adjusts placeholder and suggestions, and recalculates destination command.
  - `searchCategoryAutocomplete(query, category)`: Scopes search to the active category.
- **Command & Script Generation**:
  - `buildMoveCommandsForItem(item, category, species)`: Correctly passes `--category "${category}"` and optional `--species "${newSlug}"` to `avicull correct`.
  - In hierarchy mode:
    - If `species` is empty: target is `birds_hierarchy/${category}`.
    - If `species` is provided: target is `birds_hierarchy/${category}/${species}`.
    - If `No Subject`: `birds_hierarchy/_No_Bird_Detected` (or `_No_Subject_Detected`).
  - Batch script template (`apply_corrections.sh`) updated to propagate `--category` parameter uniformly.

### D. CLI & Core Organizer Logic (`src/organizer/correct.rs` and `src/cli.rs`)
- In `src/cli.rs`:
  - Update `CorrectArgs`:
    ```rust
    /// Correct category (e.g. "Bird", "Mammalia", "Insecta", "Scenery")
    #[arg(short = 'c', long)]
    pub category: String,

    /// Correct species name or sub-folder (optional: leave empty for top-level category filing)
    #[arg(short, long, default_value = "")]
    pub species: String,
    ```
- Update `apply_bundle_correction_with_category`:
  - When `--category` is explicitly provided, validate and normalize it against `AssetCategory`.
  - Parse `--species`: if empty or whitespace, treat as `entity_slug: None` (top-level kingdom/genre).
  - Update `BundleDecision`:
    - `classification`: `Classification { category, entity_slug }`.
    - `is_user_override`: set to `true`.
  - When category is non-avian, suppress avian typo suggestions.
  - In `--mode hierarchy`:
    - If `entity_slug` is `Some(s)`: link into `birds_hierarchy/<category>/<s>/<file>`.
    - If `entity_slug` is `None`: link into `birds_hierarchy/<category>/<file>`.
  - In `--mode in-place-rename`:
    - If `entity_slug` is `Some(s)`: format as `<stem>_<category>_<s>.<ext>`.
    - If `entity_slug` is `None`: format as `<stem>_<category>.<ext>` (canonically parsed by `parse_inplace_filename`).
  - In `--mode in-place`: Save record in manifest; do not alter source files.

### E. SQLite Manifest & Preservation (`src/storage/local.rs` and `src/db.rs`)
- `bundles` and `bundle_decisions` tables already feature `category TEXT` and `assigned_species_slug TEXT`.
- When `entity_slug` is `None`, `assigned_species_slug` is stored as `""` (or NULL), and `category` stores the canonical string representation (e.g. `"Mammalia"`, `"Scenery"`).
- Reclassification precedence strictly honors non-avian user overrides during delta scans.

---

## 5. Mode Behavior Matrix

| Organization Mode | Filesystem Action on Correction | Manifest Action |
| :--- | :--- | :--- |
| **`--mode in-place`** (Default) | **Zero disk modification**; source files remain read-only | Updates `manifest.sqlite` with `category`, `assigned_species`, `is_user_override = 1` |
| **`--mode in-place-rename`** | Atomically renames source file & sidecars with `<Category>_<Species>` token | Updates `manifest.sqlite` and records rename journal in `in_place_rename` table |
| **`--mode hierarchy`** | Creates `birds_hierarchy/<Category>/<Species>/` (or `<Category>/` for root genre) and relinks asset bundle hard links | Updates `manifest.sqlite` with new target path and `is_user_override = 1` |

---

## 6. What's Left to the Builder
- Precise CSS styling of the category pills to match avicull dark theme.
- Number of suggestions displayed in dropdown (default 15).
- Keyboard navigation inside the suggestion dropdown (Up/Down arrow keys, Enter to select).
- Consolidation of tests under `tests/test_suite/` verifying multi-category corrections across all three organization modes.
