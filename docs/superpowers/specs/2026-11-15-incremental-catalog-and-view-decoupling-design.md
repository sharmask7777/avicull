# Strictly Incremental Catalog Generation, State-View Decoupling & Fast Re-classification Design

**Document:** Design Specification  
**Path:** `docs/superpowers/specs/2026-11-15-incremental-catalog-and-view-decoupling-design.md`  
**Date:** November 15, 2026  
**Status:** Approved by User  

---

## 1. Context & Motivation

High-volume avian photo archives (50–200 trip directories, 50k–250k RAW+JPEG files, 2–10 TB) are frequently stored on Network-Attached Storage (NAS) mounts (Synology, QNAP, TrueNAS) over SMB3 or NFSv4.
Previously, running `avicull organize-all` on an updated archive took **hours or days** because:
1. **Catastrophic 9 TB RAW Triple-Read Bug**: In the root parent rollup, preview path lookups failed because child previews lived in `<parent>/<child>/.avicull/previews/` rather than `<parent>/.avicull/previews/`. This caused the parent pass to re-read every 30 MB RAW file over SMB up to 3 times per asset (9 TB total over the wire).
2. **Per-Stat Network Latency**: Traversing 100k files over SMB incurred 1–5 ms per file `stat`, spending 10–15 minutes on network RPCs before processing started.
3. **The 2.66 GB HTML Bomb**: Embedding base64 micro-thumbnails for 100,000 photos produced a 2.66 GB HTML document, crashing browser V8 engines with Out-Of-Memory errors.
4. **Coupled State and View**: Modifying HTML templates or UI layout required re-scanning disk and re-running neural classification.

---

## 2. Design Invariants & User Directives

1. **Unhidden Catalog Asset Directory (`avicull_catalog_files`)**:
   - Replace the hidden `.avicull` directory with `avicull_catalog_files` (no leading dot).
   - Contains:
     - `avicull_catalog_files/manifest.sqlite` (SQLite manifest, sync states, embeddings)
     - `avicull_catalog_files/previews/{stem}.jpg` (precomputed 1200px previews)
   - Traversal scanners (`should_skip_directory`, `scan_input_tree_with_skip`) strictly skip `avicull_catalog_files` and legacy `.avicull`.
   - Backward-compatible: Reads existing `.avicull/` if `avicull_catalog_files/` is not yet present.
2. **Directory-Only Walk & Composite Signature Change Detection**:
   - Skips file entries during change detection; traverses directory inodes only (depth 8, 10–50 folders, $< 20\text{ ms}$).
   - Computes:
     $$T_{\text{max}} = \max_{d \in \text{dirs}} \text{mtime}(d), \quad N_{\text{entries}} = \sum_{d \in \text{dirs}} \text{readdir\_count}(d)$$
   - If signature matches `directory_sync_state` in SQLite and `catalog.html` exists $\implies$ Skip child directory in $< 2\text{ ms}$ ($\le 3$ RPCs).
3. **Zero-Image-I/O Parent Rollup with Dual Thumbnail Strategy**:
   - In parent rollup, base64 micro-thumbnails are suppressed (`embedded_thumb_data_uri = None`).
   - Previews link to `{child}/avicull_catalog_files/previews/{stem}.jpg`.
   - In the HTML catalog template, previews are **lazy loaded** via `loading="lazy"` on `<img>` elements and an IntersectionObserver in the client-side virtual grid.
   - Parent catalog size drops from **2.66 GB to 28 MB**, loading in $< 400\text{ ms}$.
   - Parent database compilation is staged in `/tmp/` on local NVMe SSD to avoid network locking.
4. **Decoupled View Renderer (`avicull render-catalog`)**:
   - CLI command: `avicull render-catalog` (alias `catalog`).
   - Re-renders `catalog.html` directly from `avicull_catalog_files/manifest.sqlite` in $< 1.5\text{ s}$ with zero image I/O.
   - Batch view-only update: `avicull organize-all --view-only`.
5. **Fast Software Re-classification (`--reclassify-mode fast`)**:
   - Reclassification mode: `ReclassifyMode::Fast` (Default) vs `ReclassifyMode::Forced`.
   - `Fast`: Re-evaluates species assignments, eBird taxonomy, location priors, and thresholds directly from stored 512-d visual embeddings (`visual_embedding BLOB`) and raw model posteriors in SQLite in pure Rust. Takes $< 1.5\text{ s}$ for 50k photos with **0 RAW reads and 0 neural network calls**.
   - `Forced`: Full neural network re-run (YOLO detector + ONNX backbone).

---

## 3. Data Contracts & Schemas

### 3.1 SQLite Tables

```sql
-- Tracks directory signatures for instant change detection
CREATE TABLE IF NOT EXISTS directory_sync_state (
    path_key TEXT PRIMARY KEY,          -- "." for trip root
    max_mtime_secs INTEGER NOT NULL,    -- Maximum mtime of any subdirectory
    total_entry_count INTEGER NOT NULL, -- Total readdir entries across subdirs
    dir_node_count INTEGER NOT NULL,    -- Number of directory nodes
    last_scanned_at TEXT NOT NULL       -- ISO 8601 timestamp
);

-- Tracks child manifest synchronization for parent rollup
CREATE TABLE IF NOT EXISTS parent_child_sync (
    child_rel_path TEXT PRIMARY KEY,    -- e.g. "2024_05_Kaziranga"
    manifest_path TEXT NOT NULL,        -- Relative path to child manifest
    child_last_run_id TEXT NOT NULL,    -- Run ID of the imported run
    imported_at TEXT NOT NULL,          -- ISO 8601 timestamp
    bundle_count INTEGER NOT NULL,      -- Count of bundles imported
    deep_mtime_secs INTEGER NOT NULL    -- Directory max mtime at sync time
);
```

### 3.2 CLI Command Definitions

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum ReclassifyMode {
    /// Re-score species assignments and priors from stored visual embeddings in SQLite (Zero ML inference)
    #[default]
    Fast,
    /// Force full neural network detection and classification on raw image files
    Forced,
}

#[derive(Args, Debug)]
pub struct RenderCatalogArgs {
    /// Source directory containing avicull_catalog_files/manifest.sqlite
    #[arg(short, long, default_value = ".")]
    pub input: PathBuf,

    /// Destination path for catalog HTML (defaults to <INPUT>/catalog.html)
    #[arg(short, long)]
    pub output_file: Option<PathBuf>,

    /// Custom gallery title display name
    #[arg(short, long)]
    pub trip_name: Option<String>,

    /// Organization mode override
    #[arg(short, long)]
    pub mode: Option<OrganizationMode>,

    /// Omit base64 micro-thumbnails to minimize HTML file size
    #[arg(long, default_value_t = false)]
    pub no_thumbnails: bool,
}
```

---

## 4. Test-Driven Verification Plan (`AGENTS.md`)

- **Phase 1**: Unit & integration tests for `DirectorySignature` probe, nested folder change detection, and `avicull_catalog_files` folder resolution.
- **Phase 2**: Parent rollup zero-image I/O tests (`chmod 000` on RAW files) and thumbnail suppression tests.
- **Phase 3**: Fast software re-classification tests from stored embeddings with 0 RAW reads.
- **Phase 4**: Standalone `render-catalog` command and Playwright lazy-loading verification.
- **Phase 5**: Full 7-stage verification via `./scripts/verify.sh`.
