# Design Document: In-Place Rename Mode & Interactive HTML Catalog

- **Date**: 2026-10-25
- **Status**: Proposed
- **Issue Reference**: [sharmask7777/avicull#5](https://github.com/sharmask7777/avicull/issues/5)
- **Author**: Antigravity & Pair Programming Partner

---

## 1. Intent & Problem Statement

`avicull` currently organizes photos by materializing a zero-byte APFS hard link hierarchy (`birds_hierarchy/<Species>/...`). While this consumes 0 additional disk payload locally on APFS, cloud storage sync clients (such as Google Drive, Proton Drive, Dropbox, and OneDrive) do not preserve POSIX hard links. Instead, cloud sync engines treat each hard link as a distinct file, re-uploading and replicating the image payloads in the cloud, resulting in massive storage bloat and sync delays.

To solve this, `avicull` will introduce an extensible organization mode system where the new default mode is **In-Place File Renaming**, accompanied by a rich, self-contained **HTML Catalog** for browsing, filtering, and correcting photo classifications.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Extensible Organization Modes (`--mode <MODE>`)**:
   - `in-place` (`InPlaceFileRenameToDescriptiveNames`, **new default**): Renames photos in their original directories without creating duplicate directory hierarchies.
   - `hierarchy` (`HierarchicalHardlinkHierarchy`): Preserves existing zero-byte APFS hard link species tree behavior.
   - Designed with an extensible trait/enum to easily add future modes (e.g. symlink tree, EXIF-tagging-only, JSON-manifest-only).
2. **In-Place Multi-Asset Atomicity**:
   - Renames primary RAW/JPEG files alongside all associated sidecars (`.xmp`, `.CR2.xmp`) and companion JPEGs in lockstep.
   - Naming convention: `<original_stem>_<Species>_<Date>_<Location>.<ext>`.
   - Manifest tracking: SQLite manifest persists original path $\rightarrow$ renamed path mapping to support idempotency and rollback.
3. **Interactive Standalone HTML Catalog (`catalog.html`)**:
   - Automatically generated in the trip root.
   - 100% self-contained single-page dashboard: zero external CDN dependencies, works completely offline.
   - Displays trip statistics (species counts, detection distributions, needs review tallies).
   - Interactive instant search and filter (by species, location, date, confidence).
   - Cloud eviction tolerance: For photos offloaded to cloud storage, displays full path and metadata cleanly without broken layout.
4. **Browser-Assisted Mislabel Correction**:
   - Search bar with client-side autocomplete against 11,167 eBird species.
   - Generates exact Unix `mv` commands for terminal execution with a one-click copy button.
   - Supports batch export as a downloadable `apply_corrections.sh` script.

### Non-Goals
- Running a background HTTP daemon or listening on localhost network sockets by default. The HTML file works directly via `file://`.
- Modifying RAW image bytes, bitstreams, or EXIF metadata in-place.
- Deleting or altering unclassified non-bird files.

### Anti-Goals
- Never leave orphaned sidecars (e.g., renaming `IMG_001.CR2` while leaving `IMG_001.xmp` with the old stem).
- Never corrupt file names on repeated runs (idempotency guard against double-suffixing).

---

## 3. Command-Line Interface & Extensible Architecture

### CLI Mode Flag
```bash
# Default mode: In-place renaming + catalog generation
avicull organize --input /Volumes/Photos/Trips/Leh

# Explicitly specifying mode
avicull organize --input /Volumes/Photos/Trips/Leh --mode in-place
avicull organize --input /Volumes/Photos/Trips/Leh --mode hierarchy
```

### Architecture
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default, Serialize, Deserialize)]
pub enum OrganizationMode {
    #[default]
    #[value(name = "in-place", alias = "in-place-file-rename-to-descriptive-names")]
    InPlaceFileRenameToDescriptiveNames,

    #[value(name = "hierarchy", alias = "hierarchical-hardlink-hierarchy", alias = "hardlink")]
    HierarchicalHardlinkHierarchy,
}
```

The organizer pipeline delegates to a strategy handler based on the active mode:
- `InPlaceRenamer`: Renames files in-place on disk and records renames in SQLite.
- `HardlinkOrganizer`: Creates hard links in `<output>/<Species>/`.
- Both modes generate or update `catalog.html` in the root.

---

## 4. In-Place Renaming Specification

### Suffix Nomenclature
The destination stem format is:
```
<original_stem>_<Species_Common_Name>[_<YYYYMMDD>][_<Location_Slug>].<ext>
```
- Capture date and location are included when present, and cleanly omitted if unavailable (no ugly `Unknown` sentinel tokens).
- Confidence scores are preserved in SQLite and displayed on `catalog.html` badges, but omitted from the file stem for clean human-readable filenames.

Example:
- Source: `IMG_4821.CR2` and `IMG_4821.CR2.xmp`
- Target: `IMG_4821_House_Sparrow_20261025_Leh.CR2` and `IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp`

If species is `_No_Bird_Detected` or `_Needs_Review`, the slug reflects the routing tier:
- `IMG_4822_Needs_Review_Black_Redstart_20261025_Leh.CR2`
- `IMG_4823_No_Bird_Detected_20261025_Leh.CR2`

### Idempotency & Collision Handling
- If a file already ends with `_<Species>_<Date>_<Location>`, `avicull` detects that it has already been processed and skips re-renaming.
- If a target filename already exists on disk, `_1`, `_2` numbered disambiguators are appended before the extension.

### Atomic Sidecar Renaming
When a primary file is renamed, all associated sidecars (`.xmp`, `.CR2.xmp`) and companion JPEGs in the same folder are renamed in lockstep with the identical base stem.

---

## 5. Location of Manifest & `catalog.html` Per Mode

- **In-Place Mode (`--mode in-place`, default)**:
  - Files are renamed in place inside `--input`.
  - SQLite database is stored at `<input>/.avicull/manifest.sqlite`.
  - HTML catalog is stored at `<input>/catalog.html`.
  - If `--output` is explicitly passed, `avicull` outputs an informational notice that in-place mode operates on `--input`.
- **Hierarchy Mode (`--mode hierarchy`)**:
  - Source directory `--input` remains strictly immutable.
  - Zero-byte hard links are materialized into `<output>` (defaults to `<input>/birds_hierarchy`).
  - SQLite database is stored at `<output>/.avicull/manifest.sqlite`.
  - HTML catalog is stored at `<output>/catalog.html`.

---

## 6. Rollback & `clean` Semantics

- The SQLite database records the `operation_type` (`in_place_rename` vs `hardlink`), storing `original_path` and `current_path`.
- When running `avicull clean`:
  - For hardlink runs: Unlinks the generated hardlinks in `<output>` (as before).
  - For in-place runs: Reverses `current_path -> original_path` via `std::fs::rename`, safely restoring original filenames for the primary files, sidecars, and companions without data loss.

---

## 7. RAW File Previews in `catalog.html`

- Companion JPEGs are used directly as relative preview links.
- For standalone RAW files without a companion JPEG, `avicull` extracts the embedded thumbnail stream using its existing zero-copy `extract_embedded_jpeg_preview` engine and saves a lightweight thumbnail in `.avicull/previews/<stem>.jpg`.
- If a photo has been offloaded/evicted to cloud storage (stub file without readable bytes), `catalog.html`'s image `onerror` handler automatically renders a sleek metadata tile displaying its relative path, species badge, and capture details.

---

## 8. Interactive Mislabel Correction Script & Shell Generation

- In `catalog.html`, clicking "Correct Species" opens an autocomplete search bar covering all 11,167 eBird species.
- Upon selecting a new species:
  - The page computes the new destination path relative to the trip root.
  - It outputs exact Unix commands referencing the current on-disk filename:
    ```bash
    mv "Leh/Day1/IMG_4821_House_Sparrow_20261025_Leh.CR2" "Leh/Day1/IMG_4821_Tibetan_Snowcock_20261025_Leh.CR2"
    mv "Leh/Day1/IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp" "Leh/Day1/IMG_4821_Tibetan_Snowcock_20261025_Leh.CR2.xmp"
    ```
  - Provides a single-click "Copy Command" button.
  - Adds to a staged corrections list with a "Download apply_corrections.sh" button for one-step batch execution.

---

## 9. Testing Strategy

1. **Unit Tests**:
   - `test_inplace_filename_formatting`: Verifies format `<stem>_<Species>[_<Date>][_<Location>].<ext>`.
   - `test_inplace_sidecar_synchronization`: Ensures `.xmp` and companion `.jpg` stems match primary.
   - `test_inplace_idempotency`: Verifies re-running does not double-append suffixes.
   - `test_html_catalog_generation`: Verifies JSON embedding, HTML validity, and stat counts.
   - `test_clean_rollback_in_place`: Verifies `clean` reverses `current_path -> original_path` without deleting original media.
2. **Filesystem Integration Tests**:
   - Real disk directory test verifying `std::fs::rename` in-place, sidecar renaming, and SQLite database updates.
3. **CLI E2E Tests**:
   - `avicull organize --mode in-place` (default) and `avicull organize --mode hierarchy`.
   - Verifies `catalog.html` presence, non-zero file size, and accurate relative paths.
