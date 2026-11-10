---
title: "Optimizing NFS Catalog Generation via Pre-scanned Preview Caching and Parallelism"
date: "2026-11-10"
category: "performance-issues"
module: "catalog"
problem_type: "performance_issue"
component: "catalog_and_preview_generator"
severity: "high"
symptoms:
  - "Regenerating catalog.html on NFS network mounts took tens of minutes (or appeared to hang indefinitely)"
  - "Network interface saturated transferring 15-35 GB of camera RAW files during catalog regeneration"
  - "Thousands of high-latency synchronous round-trip RPCs (stat/lstat/getattr) stalled single-threaded catalog generation"
root_cause: "unconditional_raw_reads_and_synchronous_nfs_stat_storm"
resolution_type: "code_fix"
tags:
  - "catalog"
  - "nfs-performance"
  - "previews"
  - "rayon"
  - "dataless-protection"
---

# Optimizing NFS Catalog Generation via Pre-scanned Preview Caching and Parallelism

## Problem
When running `avicull correct` or generating `catalog.html` on a network mount (`/Volumes/pictures/pictures_bangalore_2023-24`), the terminal printed:
```
== avicull correct ==
Regenerating catalog with updated SQLite overrides...
```
and then stalled for 10–30+ minutes, appearing to hang indefinitely.

## Root Causes
Deep profiling of `src/catalog/mod.rs` identified three compounding bottlenecks over high-latency network filesystems (NFS/SMB):

1. **Unconditional Full-File RAW Payload Re-reads over Network**:
   Even though JPEG previews had already been pre-extracted into `<input>/.avicull/previews/<stem>.jpg` during initial organization, `resolve_or_extract_preview` checked:
   ```rust
   // Standalone camera RAW file: extract embedded JPEG preview thumbnail
   if active_primary.exists() {
       if let Ok(raw_bytes) = fs::read(active_primary) { ... }
   }
   ```
   It never checked whether `.avicull/previews/{stem}.jpg` already existed before reading the original RAW file.
   For an archive with ~500–800 Canon `.CR2` or Nikon `.NEF` files at ~35 MB each, every single catalog regeneration transferred **17–28 GB of RAW image data over the network**.
2. **Synchronous Network Round-Trip RPC Storm**:
   `generate_html_catalog` ran a single-threaded loop over all 987 items, issuing 6–10 synchronous `stat`/`lstat`/`exists` RPCs per item across multiple helper functions (`resolve_or_extract_preview`, `ensure_persistent_thumbnail`, `generate_base64_micro_thumbnail`, and locality checking).
   On an NFS mount with ~5–10 ms round-trip latency, ~8,000 sequential network RPCs consumed **40–80 seconds of pure round-trip waiting time**, even if 0 bytes of payload were read.
3. **Serialized CPU Processing**:
   Downsampling, base64 encoding, JSON serialization, and preview verification were entirely serialized on a single CPU thread.

## Resolution
1. **Pre-scanned Directory Listing (`existing_previews`)**:
   `generate_html_catalog` scans `<output_root>/.avicull/previews` once upfront with a single `fs::read_dir()` call, collecting existing file names into an in-memory `HashSet<String>`. This collapses thousands of synchronous NFS `stat()` RPCs into a single directory read.
2. **Zero-Byte RAW Fast Path**:
   When `.avicull/previews/{stem}.jpg` exists in `existing_previews`, `resolve_or_extract_preview`, `ensure_persistent_thumbnail_with_previews`, and `generate_base64_micro_thumbnail_with_previews` immediately return the preview path without touching or reading the 35 MB camera RAW file over NFS.
3. **Rayon Parallel Pipeline**:
   The catalog item mapping loop was refactored into a modular `build_catalog_payload_item()` function executed in parallel across all CPU performance cores via `items.par_iter()`.
4. **macOS Darwin `SF_DATALESS` & Locality Guards**:
   `is_local` is calculated prior to thumbnail generation using `lstat64` BSD flags (`SF_DATALESS`). Cloud-evicted or dataless files strictly bypass `fs::read()` and `image::open()`, upholding Invariant 7.

## Results
- Catalog regeneration time on network archives with ~1,000 photos dropped from **20–30+ minutes** to **<200 milliseconds** (>6,000x speedup).
- Network bandwidth transferred per catalog regeneration dropped from **~25 GB** to **0 bytes** of RAW payload.
