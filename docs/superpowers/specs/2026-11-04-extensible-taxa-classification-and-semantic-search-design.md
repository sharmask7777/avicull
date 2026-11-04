# Extensible Multi-Taxa Classification & Semantic Embedding Architecture Design

**Status**: Approved  
**Date**: 2026-11-04  
**Author**: Antigravity & User  
**Addresses**: [Issue #21 (Classification Accuracy)](https://github.com/sharmask7777/avicull/issues/21) & [Issue #40 (Extensibility to Animals & Embedded Search)](https://github.com/sharmask7777/avicull/issues/40)

---

## 1. Intent & Context

`avicull` was originally engineered as an avian photo culling and organization engine. However, wildlife, outdoor, and general photography inherently captures non-avian subjects: mammals (e.g. Malabar Giant Squirrel, deer, monkeys), insects (e.g. butterflies, dragonflies), landscape/scenery (e.g. aurora borealis, mountains, sunsets), and eventually general photography genres (portraits, street, architecture, abstract).

In [Issue #21](https://github.com/sharmask7777/avicull/issues/21), users reported that butterflies were classified as *Green Cochoa* and Malabar Giant Squirrels were tagged as *Rufous Treepie*. Investigation revealed this stems from the **Closed-World Softmax Fallacy**: the system's embedding dictionary was restricted to 11,167 bird species, forcing softmax to assign 100% of probability mass across birds even for non-bird inputs.

In [Issue #40](https://github.com/sharmask7777/avicull/issues/40), the requirement was established to refactor the architecture so it naturally extends to all animals and general-purpose embedded visual search (like Immich or Apple Photos) in the future, while ensuring that automated tests fail if a future refactor violates this extensibility.

---

## 2. Goals, Non-Goals & Anti-Goals

### Goals
1. **Accurate Open-Set Classification (Resolves Issue #21)**:
   Distinguish between high-level categories (`Bird`, `Mammalia`, `Insecta`, `Scenery`) and accurately identify specific fine-grained species or scene descriptors (e.g. `_Mammalia_malabar_giant_squirrel_`, `_Bird_white-throated_kingfisher_`, `_Insecta_malabar_banded_peacock_`, `_Scenery_aurora_borealis_`).
2. **Zero Extra Runtime Compute & Zero VRAM Bloat**:
   Must remain fully runnable on a 16GB M2 Pro MacBook (or base M1/M2 Mac). Do not add heavy multi-gigabyte secondary models (no Grounding DINO, no LLMs/VLMs). Leverage BioCLIP's existing multimodal embedding space (trained on TreeOfLife-10M / iNat21).
3. **Extensible 2-Level Nomenclature (Singular Category Format)**:
   Implement a universal, fully reversible file-naming and directory schema:
   `<stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location>].<ext>`
   Categories are singular (`Bird`, `Mammalia`, `Insecta`, `Scenery`). This schema cleanly scales from nature/wildlife to future photographic genres (e.g. `_Portrait_close_up_`, `_Abstract_light_refraction_`, `_Street_night_market_`).
4. **Persistent Multimodal Visual Embeddings (Foundation for Issue #40)**:
   Store the 512-dimensional BioCLIP embedding for every processed asset bundle directly in `.avicull/manifest.sqlite` as a compact binary `BLOB` (2,048 bytes per photo). This lays the zero-compute groundwork for instant natural-language text search (e.g. querying `"aurora borealis"` via vector dot product).
5. **Interactive Catalog Evolution (`bird_catalog.html` / `catalog.html`)**:
   Add category filter pills (`All`, `Bird`, `Mammalia`, `Insecta`, `Scenery`), card category badges, and unified search to the existing interactive HTML catalog while preserving full backward compatibility.
6. **Architectural Decoupling & Regression Guardrails**:
   Refactor bird-only assumptions into domain-agnostic `TaxonClassifier`, `TaxonomyEntry`, and `AssetCategory` abstractions. Add automated tests that enforce this extensibility so future refactors cannot restrict domains back to birds only.

### Non-Goals
- Building an interactive web-based free-text search UI or natural-language query prompt in this milestone (that will be tackled in a follow-up feature on top of the persisted embeddings).
- Re-training or fine-tuning neural backbones from scratch.
- Real-time video processing.

### Anti-Goals
- Ad-hoc string hacking or hardcoded animal blacklists/whitelists.
- Violating the core `avicull` invariants: source immutability, zero-byte APFS hard links (`st_nlink >= 2`), sub-second delta scans, and deterministic atomic rollbacks.

---

## 3. Nomenclature & File System Organization

### In-Place Mode (`--mode in-place` / default)
Files and companion sidecars (`.xmp`, companion RAW/JPEG pairs) are renamed in-place within their source directory:
```
<stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location>].<ext>
```
Categories are strictly singular: `Bird`, `Mammalia`, `Insecta`, `Scenery`.

#### Canonical Examples:
- **Birds**: `DSC_0042_Bird_white-throated_kingfisher_20261102_Western_Ghats.CR2`
- **Mammals**: `DSC_0105_Mammalia_malabar_giant_squirrel_20261102_Western_Ghats.CR2`
- **Insects**: `DSC_0088_Insecta_malabar_banded_peacock_20261102_Western_Ghats.CR2`
- **Scenery**: `DSC_0201_Scenery_aurora_borealis_20261102_Tromso.CR2`
- **Unclassified / Empty Shots**:
  - Canonical format: `DSC_0300_No_Subject_Detected_20261102.CR2`
  - Legacy format: `DSC_0300_No_Bird_Detected_20261102.CR2` (fully recognized and supported)
- **Needs Review**:
  - `DSC_0042_Needs_Review_Bird_white-throated_kingfisher_20261102.CR2`
  - `DSC_0105_Needs_Review_Mammalia_malabar_giant_squirrel_20261102.CR2`
- **Future Genres (Fully Supported by Schema)**:
  - Portraits: `IMG_5012_Portrait_headshot_20261102_Studio.CR2`
  - Abstract: `IMG_5020_Abstract_glass_refraction_20261102.JPG`
  - Street / Architecture: `IMG_5035_Architecture_gothic_cathedral_20261102_Paris.CR2`

#### Reconciler & Parser Invariant:
The slug parser in `src/organizer/reconcile.rs` uses a known category registry (`Bird`, `Birds`, `Mammalia`, `Insecta`, `Scenery`, `Portrait`, `Abstract`). Legacy single-level bird files (e.g. `DSC_0042_Painted_Stork.CR2`) are recognized and seamlessly promoted without corruption.

### Hierarchy Mode (`--mode hierarchy`)
Output directory tree (`birds_hierarchy/` or `--output <PATH>`):
```
birds_hierarchy/
├── Bird/
│   ├── White_throated_Kingfisher/
│   │   └── DSC_0042.CR2 -> (hard link to source)
│   └── Green_Cochoa/
├── Mammalia/
│   ├── Malabar_Giant_Squirrel/
│   │   └── DSC_0105.CR2 -> (hard link to source)
│   └── Lion/
├── Insecta/
│   └── Malabar_Banded_Peacock/
├── Scenery/
│   └── Aurora_Borealis/
├── _Needs_Review/
│   ├── Bird/
│   ├── Mammalia/
│   └── Insecta/
└── _No_Subject_Detected/  (with symlink/alias to _No_Bird_Detected/ for backward compatibility)
```

---

## 4. Technical Architecture & Algorithm

### 4.1 Hierarchical Taxonomy & Embedding Dictionary
The taxonomy dictionary is structured into typed hierarchical records:
```json
{
  "taxon_id": "mamm_mal_sq1",
  "category": "Mammalia",
  "common_name": "Malabar Giant Squirrel",
  "scientific_name": "Ratufa indica",
  "folder_slug": "Malabar_Giant_Squirrel"
}
```

Precomputed embeddings:
- **Birds (`Bird`)**: 11,167 species from Cornell eBird / Clements taxonomy.
- **Mammals (`Mammalia`)**: ~6,500 species from global IUCN / iNaturalist taxonomy.
- **Insects (`Insecta`)**: Prominent global butterflies, moths, dragonflies (Lepidoptera, Odonata).
- **Scenery (`Scenery`)**: Standard photographic scenery motifs (aurora borealis, mountain, waterfall, sunset, forest, beach, desert, night sky).

**Storage Footprint**: $22,000 \times 512 \times 4 \text{ bytes} \approx 45 \text{ MB}$ (stored in `models/taxa_embeddings.safetensors`).  
**RAM Impact on M2 Pro**: ~45 MB total, trivial on a 16GB machine.

### 4.2 Two-Stage Hierarchical Classification

```
                    [ Input Image Crop ]
                             │
                             ▼
                 [ BioCLIP Vision Backbone ]
                 (Extracts 512-dim embedding z)
                             │
                             ▼
              [ Stage 1: Category Anchor Gating ]
     Dot product against high-level category centroids:
         s_bird = z · e_Bird
         s_mamm = z · e_Mammalia
         s_ins  = z · e_Insecta
         s_scen = z · e_Scenery
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
   Target Category Identified           Ambiguous Margin
   (e.g. s_mamm > s_bird + δ)          (|s_a - s_b| < δ)
            │                                 │
            ▼                                 ▼
[ Stage 2: Fine-Grained Search ]     [ Route to _Needs_Review ]
Query only candidates in matched     Flagged with top candidate
category (e.g. Mammalia space)       for human confirmation
```

1. **Category Gating (Stage 1)**:
   Evaluates dot product of image embedding $\mathbf{z}$ against precomputed category anchor vectors $\mathbf{e}_{\text{cat}}$.
   - If $\max(\mathbf{s}_{\text{non-bird}}) > s_{\text{bird}} + \delta$, the subject is classified under the winning non-avian category.
   - If $s_{\text{bird}} > \max(\mathbf{s}_{\text{non-bird}}) + \delta$, the subject proceeds to fine-grained avian species ranking.
2. **Fine-Grained Species Ranking (Stage 2)**:
   The dot product is evaluated against candidate species in that specific category, combined with geospatial priors when available.
3. **Detector Crop Expansion & Fallback Gating**:
   YOLO generates bounding crops for birds and general COCO animals (`cat`, `dog`, `horse`, `sheep`, `cow`, `elephant`, `bear`, `zebra`, `giraffe`). When 0 objects are detected (`crop is None`):
   - Full frame is embedded via BioCLIP.
   - If $s_{\text{scenery}} > s_{\text{fauna}}$, it is tagged as `Scenery_<motif>`.
   - If confidence is low, it routes cleanly to `_No_Subject_Detected` with confidence 0.0, **never** guessing a random bird.

### 4.3 SQLite Schema Migration for Semantic Search Foundation
In `.avicull/manifest.sqlite`:
```sql
ALTER TABLE bundle_decisions ADD COLUMN category TEXT NOT NULL DEFAULT 'Bird';
ALTER TABLE bundle_decisions ADD COLUMN visual_embedding BLOB;
```
- `category`: Text string (`'Bird'`, `'Mammalia'`, `'Insecta'`, `'Scenery'`, `'Portrait'`, etc.).
- `visual_embedding`: Compact 2,048-byte binary blob (512 IEEE-754 single-precision floats).
- **Extensibility Search Query**: In-memory vector dot product across stored blobs executes in $<5\text{ ms}$ across 10,000 photos on Apple Silicon.

### 4.4 Interactive Catalog Evolution (`bird_catalog.html` / `catalog.html`)
1. **Category Pills**: Added to the filters bar with dynamic item counts:
   ```html
   [ All ]  [ Bird (38) ]  [ Mammalia (8) ]  [ Insecta (4) ]  [ Scenery (12) ]  │  [ Needs Review (5) ]
   ```
2. **Card Category Badges**: Cards display `<Category> • <Subject>` (e.g., `Mammalia • Malabar Giant Squirrel`).
3. **Unified Search**: Search bar indexes both `category` and `assigned_species`.
4. **Correction Modal**: Autocompletes across all taxa and outputs the 2-level `avicull correct --category <CAT> --species <SPEC>` command.

---

## 5. Extensibility Regression Tests & Guardrails

To fulfill the requirements of Issue #40, automated test suites will enforce:
1. **Generic Classifier Trait**: `src/classifier.rs` defines a generic `TaxonClassifier` trait where inputs accept arbitrary taxonomic domains, not hardcoded avian types.
2. **Universal Embedding Storage**: Test suite verifies that any asset bundle processed by the pipeline retains a valid 512-dim embedding in SQLite.
3. **Multi-Domain Nomenclature Integrity**: Unit tests verify that `format_inplace_filename` and `parse_inplace_filename` round-trip cleanly for `Bird`, `Mammalia`, `Insecta`, `Scenery`, `Portrait`, and `Abstract`.
4. **Backward Compatibility**: Test verifies that existing archives named `_No_Bird_Detected` or legacy bird stems are cleanly recognized without data loss.
5. **Breaking Change CI Gate**: Any commit that restricts `TaxonClassifier` or database records back to bird-only types will fail the test suite.
