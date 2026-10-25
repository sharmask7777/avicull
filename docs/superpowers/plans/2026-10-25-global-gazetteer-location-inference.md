# Global Gazetteer & Directory Path Location Inference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable `avicull` to automatically infer geographic coordinates for photo bundles missing EXIF GPS by parsing directory paths against an embedded global gazetteer, interactively disambiguating candidates, and applying an eBird regional checklist prior to eliminate out-of-region misclassifications (such as Bald Eagles or Black Swans in South India).

**Architecture:** A path parser extracts location candidate tokens while stripping structural noise directories and date patterns. An in-memory gazetteer (built from global eBird Hotspots and GeoNames) performs fast trigram fuzzy matching to present 3–4 ranked candidates in interactive terminal runs. Confirmed locations are cached in SQLite (`.avicull/manifest.sqlite`), and resolved regional codes (`IN-KA`, `IN-RJ`, etc.) drive an eBird regional checklist matrix during Bayesian classification to downrank non-occurring species.

**Tech Stack:** Rust (2021 edition), `rusqlite`, `serde`, `serde_json`, `clap`, Python 3 (`scripts/build_gazetteer.py`, `scripts/infer.py`), SQLite.

**Spec:** [`docs/superpowers/specs/2026-10-25-global-gazetteer-location-inference-design.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/superpowers/specs/2026-10-25-global-gazetteer-location-inference-design.md)

## Global Constraints

- **Strict Source Immutability**: The input archive (`--input`) is strictly read-only; never modify, rename, or touch files in `--input`.
- **Zero-Byte Output Tree via Hard Links**: All photo file references in `birds_hierarchy` must use `std::fs::hard_link`.
- **Offline-First**: Standard operation must run 100% offline with embedded datasets; online geocoding is strictly an opt-in fallback for unknown spots.
- **TDD Mandatory**: No production code without a failing test first. Every step follows RED -> GREEN -> REFACTOR.
- **Apple Silicon Performance**: In-memory gazetteer lookups must execute in < 5ms without exceeding 25 MB RAM.

## Review Focus

1. **Path Traversal with Deeply Nested Noise**: Folders like `Pictures/2025/Bangalore/Trip 1/RAW/Selects/100CANON` must correctly extract `"Bangalore"` and `"Trip 1"` while rejecting all structural and date tokens.
2. **Interactive TTY vs. Non-Interactive Pipes**: When `stdin` is not a TTY (pipes, CI, background tasks), prompts must not block indefinitely; they must fall back to top candidates (if score > 0.85) or proceed without spatial priors.
3. **Punctuation and Diacritics in Place Names**: Locations like `Saul Kere`, `Ranthambhore / Ranthambore`, and `São Paulo` must normalize cleanly for fuzzy matching.
4. **Out-of-Region Penalty Stability**: Species outside the eBird regional checklist must be penalized to 0.001 without zeroing out logit probabilities or causing NaN/underflow in softmax distributions.
5. **Sibling EXIF Precedence Integrity**: If even a single photo in a trip folder has camera EXIF GPS, sibling propagation must take precedence and suppress gazetteer prompting.

---

### Task 1: Path Noise Filter & Candidate Token Extractor

**Files:**
- Create: `src/location/mod.rs`
- Create: `src/location/path_parser.rs`
- Modify: `src/lib.rs:1-30`
- Test: `tests/test_path_parser.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn extract_candidate_location_tokens(path: &std::path::Path) -> Vec<String>;
  pub fn is_structural_noise_token(token: &str) -> bool;
  ```

- [ ] **Step 1: Write the failing tests for path parsing and noise filtering**

```rust
// tests/test_path_parser.rs
use avicull::location::path_parser::{extract_candidate_location_tokens, is_structural_noise_token};
use std::path::Path;

#[test]
fn test_noise_token_detection() {
    assert!(is_structural_noise_token("raw"));
    assert!(is_structural_noise_token("RAW"));
    assert!(is_structural_noise_token("processed"));
    assert!(is_structural_noise_token("selects"));
    assert!(is_structural_noise_token("100canon"));
    assert!(is_structural_noise_token("2025-04-19"));
    assert!(is_structural_noise_token("apr 19 2026"));
    assert!(is_structural_noise_token("oct 2025"));
    assert!(!is_structural_noise_token("bangalore"));
    assert!(!is_structural_noise_token("saul kere"));
    assert!(!is_structural_noise_token("ranthambore"));
}

#[test]
fn test_extract_candidate_tokens_from_nested_path() {
    let path = Path::new("/Users/photos/pictures_bangalore-2025-/saul kere apr 19 2026/RAW/1X7A5242.CR2");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["saul kere", "bangalore"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_path_parser`
Expected: FAIL (cannot find module `location`)

- [ ] **Step 3: Implement `extract_candidate_location_tokens` and `is_structural_noise_token`**

In `src/location/path_parser.rs`, parse path components from leaf to root, strip dates with regex / date parsers, filter out structural directories (`raw`, `processed`, `export`, `selects`, `dcim`, `100canon`), split punctuation into cleaned phrases, and deduplicate. Expose `pub mod location;` in `src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_path_parser`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/location/mod.rs src/location/path_parser.rs src/lib.rs tests/test_path_parser.rs
git commit -m "feat(location): add path noise filtering and candidate token extractor"
```

---

### Task 2: Global Gazetteer & Regional Checklist Builder

**Files:**
- Create: `scripts/build_gazetteer.py`
- Create: `data/gazetteer.json`
- Create: `data/regional_checklists.json`
- Test: `tests/test_build_gazetteer.py`

**Interfaces:**
- Produces:
  - `data/gazetteer.json`: Array of `{ "name": str, "region_code": str, "lat": float, "lon": float, "category": "hotspot" | "city" | "reserve" }`
  - `data/regional_checklists.json`: Map of `{ region_code: [taxon_id, ...] }`

- [ ] **Step 1: Write failing verification test for gazetteer data integrity**

```python
# tests/test_build_gazetteer.py
import json
import os

def test_gazetteer_contains_key_global_and_regional_hotspots():
    gazetteer_path = os.path.join(os.path.dirname(__file__), "..", "data", "gazetteer.json")
    assert os.path.exists(gazetteer_path), "gazetteer.json must exist"
    
    with open(gazetteer_path, "r", encoding="utf-8") as f:
        data = json.load(f)
        
    names = {entry["name"].lower(): entry for entry in data}
    # Check Indian & Bangalore spots
    assert "saul kere" in names or "saul kere lake" in names
    assert "bengaluru" in names or "bangalore" in names
    assert "ranthambore" in names or "ranthambhore" in names
    assert "leh" in names
    # Check Global spots
    assert "everglades" in names or "everglades national park" in names
    assert "kruger" in names or "kruger national park" in names
    assert "central park" in names
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest tests/test_build_gazetteer.py`
Expected: FAIL (file does not exist)

- [ ] **Step 3: Implement `scripts/build_gazetteer.py` to generate `gazetteer.json` and `regional_checklists.json`**

Implement `scripts/build_gazetteer.py` using Python standard library:
1. Compile global birding hotspots (eBird + IBA + WDPA national parks) and major global cities into `data/gazetteer.json`.
2. Map regional codes (`IN-KA`, `IN-RJ`, `IN-LA`, `US-FL`, `KE-01`, etc.) to confirmed species taxon IDs from `models/taxonomy.json` into `data/regional_checklists.json`.
3. Execute `python3 scripts/build_gazetteer.py`.

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest tests/test_build_gazetteer.py`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/build_gazetteer.py data/gazetteer.json data/regional_checklists.json tests/test_build_gazetteer.py
git commit -m "feat(data): add global gazetteer and eBird regional checklist builder"
```

---

### Task 3: In-Memory Gazetteer Engine & Trigram Fuzzy Matcher

**Files:**
- Create: `src/location/gazetteer.rs`
- Modify: `src/location/mod.rs`
- Test: `tests/test_gazetteer.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  pub struct GazetteerEntry {
      pub name: String,
      pub region_code: String,
      pub latitude: f64,
      pub longitude: f64,
      pub category: String,
  }

  pub struct GazetteerEngine { /* ... */ }

  impl GazetteerEngine {
      pub fn load_embedded() -> anyhow::Result<Self>;
      pub fn search(&self, query: &str, limit: usize) -> Vec<(GazetteerEntry, f64)>;
  }
  ```

- [ ] **Step 1: Write failing unit test for gazetteer fuzzy search**

```rust
// tests/test_gazetteer.rs
use avicull::location::gazetteer::GazetteerEngine;

#[test]
fn test_gazetteer_search_exact_and_fuzzy() {
    let engine = GazetteerEngine::load_embedded().expect("should load embedded gazetteer");
    
    // Search exact hotspot
    let matches = engine.search("saul kere", 4);
    assert!(!matches.is_empty());
    assert!(matches[0].0.name.to_lowercase().contains("saul kere"));
    assert_eq!(matches[0].0.region_code, "IN-KA");
    
    // Search city name with typo/variant
    let matches_blr = engine.search("bangalore", 4);
    assert!(!matches_blr.is_empty());
    assert!(matches_blr.iter().any(|(e, _)| e.region_code == "IN-KA"));
    
    // Search international reserve
    let matches_kruger = engine.search("kruger", 4);
    assert!(!matches_kruger.is_empty());
    assert!(matches_kruger[0].0.name.to_lowercase().contains("kruger"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_gazetteer`
Expected: FAIL (module `gazetteer` not found)

- [ ] **Step 3: Implement `GazetteerEngine` with embedded JSON and Trigram matcher**

In `src/location/gazetteer.rs`, embed `data/gazetteer.json` via `include_str!`. Implement trigram string similarity:
$$S(A, B) = \frac{2 \cdot |tri(A) \cap tri(B)|}{|tri(A)| + |tri(B)|}$$
Boost exact substring occurrences and hotspot categories. Rank candidates and return the top matches.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_gazetteer`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/location/gazetteer.rs src/location/mod.rs tests/test_gazetteer.rs
git commit -m "feat(location): implement in-memory gazetteer engine with trigram matcher"
```

---

### Task 4: Interactive Disambiguation Prompt & SQLite Location Cache

**Files:**
- Create: `src/location/resolver.rs`
- Modify: `src/manifest.rs:1-120`
- Modify: `src/location/mod.rs`
- Test: `tests/test_location_resolver.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ResolvedLocation {
      pub name: String,
      pub region_code: String,
      pub latitude: f64,
      pub longitude: f64,
      pub source: String,
  }

  pub fn resolve_trip_locations<R: std::io::BufRead, W: std::io::Write>(
      trips_to_tokens: &[(String, Vec<String>)],
      engine: &GazetteerEngine,
      manifest: &crate::manifest::ManifestDb,
      reader: &mut R,
      writer: &mut W,
      is_interactive: bool,
      location_override: Option<&str>,
  ) -> anyhow::Result<std::collections::HashMap<String, ResolvedLocation>>;
  ```

- [ ] **Step 1: Write failing tests for SQLite caching and disambiguation resolution**

```rust
// tests/test_location_resolver.rs
use avicull::location::gazetteer::GazetteerEngine;
use avicull::location::resolver::{resolve_trip_locations, ResolvedLocation};
use avicull::manifest::ManifestDb;
use tempfile::TempDir;
use std::io::Cursor;

#[test]
fn test_resolver_with_interactive_input() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![
        ("pictures_bangalore-2025-/saul kere apr 19 2026".to_string(), vec!["saul kere".to_string(), "bangalore".to_string()]),
    ];

    // User chooses option 1
    let mut input = Cursor::new("1\n");
    let mut output = Vec::new();

    let resolved = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        true, // is_interactive
        None,
    ).unwrap();

    assert_eq!(resolved.len(), 1);
    let loc = resolved.get("pictures_bangalore-2025-/saul kere apr 19 2026").unwrap();
    assert!(loc.name.to_lowercase().contains("saul kere"));
    assert_eq!(loc.region_code, "IN-KA");

    // Second run: should pull from SQLite cache without asking input
    let mut empty_input = Cursor::new("");
    let mut empty_output = Vec::new();
    let cached = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut empty_input,
        &mut empty_output,
        true,
        None,
    ).unwrap();
    assert_eq!(cached.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_location_resolver`
Expected: FAIL (module `resolver` not found)

- [ ] **Step 3: Implement `directory_locations` SQLite table and `resolve_trip_locations`**

1. In `src/manifest.rs`, add table `directory_locations (path_key TEXT PRIMARY KEY, resolved_name TEXT, region_code TEXT, latitude REAL, longitude REAL, source TEXT, updated_at INTEGER)`.
2. In `src/location/resolver.rs`, check SQLite cache first. If missing:
   - If `location_override` is provided, resolve against gazetteer or parse `lat,lon`.
   - If interactive, print top 3-4 matches and read user choice (or custom string).
   - If non-interactive, pick top match if score > 0.85, else skip.
   - Save confirmed resolution to SQLite.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_location_resolver`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/location/resolver.rs src/manifest.rs src/location/mod.rs tests/test_location_resolver.rs
git commit -m "feat(location): add interactive disambiguation prompt and SQLite location caching"
```

---

### Task 5: eBird Regional Checklist Bayesian Prior Integration

**Files:**
- Modify: `src/classifier.rs:1-150`
- Modify: `scripts/infer.py:180-235`
- Test: `tests/test_regional_prior.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn get_regional_prior_multiplier(
      region_code: Option<&str>,
      taxon_id: &str,
      checklists: &std::collections::HashMap<String, std::collections::HashSet<String>>,
  ) -> f64;
  ```

- [ ] **Step 1: Write failing test for eBird regional checklist prior**

```rust
// tests/test_regional_prior.rs
use avicull::classifier::get_regional_prior_multiplier;
use std::collections::{HashMap, HashSet};

#[test]
fn test_regional_prior_penalizes_out_of_range_lookalikes() {
    let mut checklists = HashMap::new();
    let mut karnataka_birds = HashSet::new();
    karnataka_birds.insert("brakit1".to_string()); // Brahminy Kite
    karnataka_birds.insert("indcor1".to_string()); // Indian Cormorant
    checklists.insert("IN-KA".to_string(), karnataka_birds);

    // Brahminy Kite in Karnataka: valid native species
    let kite_prior = get_regional_prior_multiplier(Some("IN-KA"), "brakit1", &checklists);
    assert_eq!(kite_prior, 1.0);

    // Bald Eagle in Karnataka: not on checklist
    let bald_eagle_prior = get_regional_prior_multiplier(Some("IN-KA"), "baleag", &checklists);
    assert_eq!(bald_eagle_prior, 0.001);

    // Black Swan in Karnataka: not on checklist
    let swan_prior = get_regional_prior_multiplier(Some("IN-KA"), "blkswa", &checklists);
    assert_eq!(swan_prior, 0.001);

    // Unassigned location: neutral prior
    let unassigned_prior = get_regional_prior_multiplier(None, "baleag", &checklists);
    assert_eq!(unassigned_prior, 1.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_regional_prior`
Expected: FAIL (`get_regional_prior_multiplier` not found)

- [ ] **Step 3: Implement `get_regional_prior_multiplier` in Rust and Python**

1. Embed `data/regional_checklists.json` into `src/classifier.rs`.
2. In Bayesian score computation, multiply species visual probability by `get_regional_prior_multiplier(region_code, taxon_id, &checklists)`.
3. In `scripts/infer.py`, update prior application to check `regional_checklists.get(region_code)` and apply the $0.001$ multiplier to out-of-range birds.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_regional_prior`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/classifier.rs scripts/infer.py tests/test_regional_prior.rs
git commit -m "feat(classifier): integrate eBird regional checklist Bayesian prior"
```

---

### Task 6: CLI Integration & Update Pipeline / Pre-push Hook

**Files:**
- Modify: `src/main.rs:1-250`
- Modify: `src/geo.rs:250-353`
- Create: `.github/workflows/update_gazetteer.yml`
- Modify: `.githooks/pre-push:1-80`
- Test: `tests/test_e2e_location_workflow.rs`

**Interfaces:**
- New CLI options for `avicull organize`:
  - `--location <NAME_OR_COORDS>`: Global override for photos lacking GPS.
  - `--no-prompt`: Run non-interactively without prompting.

- [ ] **Step 1: Write failing end-to-end integration test**

```rust
// tests/test_e2e_location_workflow.rs
use tempfile::TempDir;
use std::fs;

#[test]
fn test_organize_with_location_flag_and_gazetteer() {
    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("trip_bangalore");
    let subfolder = input_dir.join("saul kere apr 19 2026");
    fs::create_dir_all(&subfolder).unwrap();

    // Create dummy image bundle
    fs::write(subfolder.join("photo1.jpg"), b"fake_jpeg").unwrap();

    // Run avicull organize with --dry-run and --location "Bangalore"
    // Assert coordinates resolved to Bangalore with source TripFolderGazetteer
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_e2e_location_workflow`
Expected: FAIL

- [ ] **Step 3: Wire location resolver into `main.rs` and update pre-push hook**

1. In `src/geo.rs` and `src/main.rs`, run `resolve_trip_locations` before classification for any trip cluster where bundles have no EXIF GPS.
2. Add `--location` and `--no-prompt` flags to `clap` CLI definitions in `src/main.rs`.
3. Create `.github/workflows/update_gazetteer.yml` with a monthly cron to re-run `python3 scripts/build_gazetteer.py`.
4. In `.githooks/pre-push`, add Stage 7: Gazetteer & Regional Checklist integrity check.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_e2e_location_workflow`
Expected: PASS

- [ ] **Step 5: Run full pre-push test suite**

Run: `.githooks/pre-push`
Expected: ALL STAGES PASS

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/geo.rs .github/workflows/update_gazetteer.yml .githooks/pre-push tests/test_e2e_location_workflow.rs
git commit -m "feat(cli): wire path location inference into organize workflow with pre-push validation"
```
