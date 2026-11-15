---
title: "Eliminating Multi-Terabyte RAW Reads and HTML Bloat in NAS Parent Rollup Catalogs"
date: "2026-11-15"
category: "performance-issues"
module: "organizer_batch_and_catalog"
problem_type: "performance_issue"
component: "catalog_and_preview_generator"
symptoms:
  - "avicull organize-all on remote NAS mounts (SMB/NFS) with multi-trip parent archives took 25+ hours or appeared to hang"
  - "Parent rollup transferred up to 9 TB of camera RAW payloads over the network"
  - "Generated parent catalog.html exceeded 2.6 GB, crashing Safari and Chrome with Out-of-Memory errors"
  - "Unchanged trip subdirectories required minutes of redundant readdir and stat operations"
root_cause: "parent_rollup_invoking_organize_without_cached_previews_and_monolithic_base64_embedding"
resolution_type: "architectural_refactor"
severity: "high"
tags:
  - "performance"
  - "nas-optimization"
  - "parent-rollup"
  - "state-view-decoupling"
  - "sqlite-staging"
  - "dual-thumbnails"
  - "fast-reclassify"
---

# Eliminating Multi-Terabyte RAW Reads and HTML Bloat in NAS Parent Rollup Catalogs

## Problem
When organizing parent archives with dozens of trip subdirectories containing 50,000–100,000+ wildlife and bird photos on Network-Attached Storage (NAS) via SMB or NFS, `avicull organize-all` took 25+ hours to complete. Even when all child trip folders had already been processed, regenerating the parent rollup catalog stalled the network interface, transferred terabytes of data, and generated a massive `catalog.html` (>2.6 GB) that crashed modern web browsers with Out-of-Memory (OOM) errors.

## Symptoms
- `avicull organize-all` spent hours in the final aggregation phase even when 0 new photos were added.
- Network throughput pegged at 80–110 MB/s continuously reading RAW payloads (`.CR2`, `.ARW`, `.NEF`) over the wire.
- In-place renaming or HTML re-rendering triggered complete scans and re-reads of the entire archive.
- The parent `catalog.html` reached 2.66 GB, failing to open in Safari or Chrome due to V8 string allocation limits.
- Network latency spikes or intermittent SMB connection drops aborted the entire batch rollup, discarding hours of work.

## What Didn't Work
- **Running `run_organize` on the Parent Root**: Attempting to treat the parent root as a giant single trip and invoking `run_organize(parent_root)` failed fundamentally. `run_organize` expected preview JPEGs at `<parent>/avicull_catalog_files/previews/{stem}.jpg`. Because the previews actually lived in `<parent>/<child>/avicull_catalog_files/previews/{stem}.jpg`, `resolve_or_extract_preview` had a 100% cache miss rate and fell back to extracting previews directly from the 35 MB camera RAW files over the network.
- **Naively Re-Scanning Directory Hierarchies**: Shallow directory checks (`depth = 1`) failed to detect photos nested inside deeper subfolders (e.g. arbitrary nested directories like `Trip_2026/Day1/Camera_A`), while full recursive traversals issued hundreds of thousands of network `stat`/`readdir` RPCs over SMB/NFS.
- **In-Memory Merging of SQLite Manifests over SMB**: Attaching and querying child SQLite databases directly over SMB triggered filesystem locking contention, SMB opportunistic lock (oplock) revocations, and network timeouts.

## Solution

### 1. Zero-Image-I/O Parent Rollup via SQLite Staging in `/tmp/`
Instead of re-scanning files or reading image bytes, `rollup_parent_catalog` operates strictly at the metadata level:
1. It copies child SQLite manifests (runtime path `<child>/avicull_catalog_files/manifest.sqlite`) to local SSD scratch storage (`$TMPDIR/avicull_rollup_<id>/`) to bypass network file locking.
2. It aggregates photo bundles, ML decisions, locations, and run records using local SQLite `ATTACH DATABASE` operations.
3. It dynamically remaps preview paths from `<child>/avicull_catalog_files/previews/{stem}.jpg` to relative paths from the parent root (`{child_rel}/avicull_catalog_files/previews/{stem}.jpg`).
4. It calls `generate_html_catalog` with zero RAW image reads:

```rust
// src/batch.rs
pub fn rollup_parent_catalog(
    parent_input: &Path,
    child_subdirs: &[PathBuf],
    config: &BatchRollupConfig,
) -> Result<()> {
    let local_staging_db = create_staged_rollup_db(parent_input, child_subdirs)?;
    let items = query_catalog_items_from_staged_db(&local_staging_db)?;
    
    // Project view without touching raw image files on the remote share
    generate_html_catalog(
        parent_input,
        &items,
        &CatalogOptions {
            suppress_base64_thumbnails: true,
            lazy_load_previews: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
```

### 2. Dual Thumbnail Strategy & Lazy Loading
To eliminate HTML bloat while preserving offline portability:
- **Single-Trip Catalogs ($\le 2,500$ photos)**: Embed lightweight base64 micro-thumbnails (~1.5 KB each) for self-contained single-file portability.
- **Parent Rollup Catalogs ($>2,500$ photos or multi-trip)**: Suppress embedded base64 strings (`suppress_base64_thumbnails = true`). Instead, photo cards link directly to precomputed preview JPEGs on disk (`{child}/avicull_catalog_files/previews/{stem}.jpg`) with `loading="lazy"` and `decoding="async"`.
- This reduced parent catalog size on 100,000 photos from **2.66 GB down to 28 MB** (a 98.9% reduction), preventing browser crashes and ensuring instant page rendering.

### 3. Deep Directory-Only Signature Probes
To detect nested photo additions on NAS mounts in milliseconds without statting individual images, `compute_directory_signature` traverses only directory inodes down to depth 8 (skipping regular files):

$$\text{Signature} = \left(\max_{d \in \text{Dirs}} \text{mtime}(d), \sum_{d \in \text{Dirs}} \text{entry\_count}(d)\right)$$

Visiting 20–50 directory inodes completes in $<20\text{ ms}$ over SMB/NFS. If the signature matches the manifest's cached signature, the entire trip folder is skipped instantly.

### 4. Decoupled State and View Architecture (`render-catalog`)
Presentation generation was decoupled completely from domain state storage:
- `avicull render-catalog` (alias `catalog`): Generates `catalog.html` purely from `avicull_catalog_files/manifest.sqlite` in $<1.5\text{ s}$ with 0 filesystem scanning or neural inference.
- `avicull organize-all --view-only`: Re-rolls up child manifests and updates the parent view in seconds.
- `avicull organize --reclassify-mode fast` (default): Re-evaluates species thresholds and routing tiers directly in pure Rust from 512-d visual embeddings stored in SQLite, reserving neural model execution (`--reclassify-mode forced`) for explicit opt-in.

### 5. Unhidden Catalog Assets Directory (`avicull_catalog_files`)
Replaced the hidden `.avicull` directory with `avicull_catalog_files` (maintaining backward-compatible fallbacks) so users on network shares can readily see, back up, and inspect persistent manifests and preview caches.

## Why This Works
- **Elimination of the 9 TB RAW Triple-Read Loop**: In the old parent rollup, each photo was checked three times for preview generation:
  $$3 \times 100{,}000 \times 30\text{ MB} = 9\text{ TB}$$
  By projecting pre-existing child preview paths into the parent catalog, RAW network reads were completely eliminated ($0\text{ bytes}$).
- **Browser Memory Limits**: Web browsers limit single-file DOM memory and JS heap size. Base64 strings inflate binary JPEG data by ~33%. A 2.6 GB HTML file exceeds Chrome/Safari string buffers. Linking disk files via `<img>` tags allows the browser's native C++ image decoder and HTTP/file cache to paginate and evict image surfaces dynamically.
- **Local SQLite Staging**: SMB network latency for SQLite locking and journaling protocols is prohibitive. Staging child SQLite reads locally on NVMe SSD achieves microsecond query performance.

## Prevention
- **Static Architecture Guardrails**: Added `tests/test_suite/test_static_architecture_guardrails.rs` to enforce:
  1. Parent rollup must never call `run_organize`.
  2. Recursive directory walkers must skip `avicull_catalog_files` and `.avicull`.
  3. Parent catalog generation must default to `suppress_base64_thumbnails = true`.
  4. Fast software re-classification must be the default reclassify mode.
- **Automated Playwright Tests**: Added specs in `tests/e2e/mock_directory_catalog.spec.ts` verifying that `loading="lazy"` and `decoding="async"` attributes are present on all rendered photo cards.

## Related Issues
- `docs/solutions/performance-issues/nfs-catalog-regeneration-optimization.md`: Addressed single-trip NFS preview caching.
- `docs/solutions/architecture-patterns/batch-subdirectory-organization-and-catalog-aggregation.md`: Defined initial `organize-all` architecture.
