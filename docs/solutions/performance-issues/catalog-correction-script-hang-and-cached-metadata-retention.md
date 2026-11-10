---
title: "Eliminating Catalog Correction Script Hangs and Preserving Scanned Metadata"
date: "2026-11-10"
category: "performance-issues"
module: "catalog,main,geo"
problem_type: "performance_issue"
component: "catalog_and_organizer"
severity: "high"
symptoms:
  - "Executing a single mislabel correction script from catalog.html hung or took an extremely long time on network shares (NFS/SMB)"
  - "Terminal output stalled after printing 'Regenerating catalog with updated SQLite overrides... Discovered 987 total photo bundles in source.'"
  - "Incremental organize re-evaluated neural network inference across all cached bundles despite no image modifications"
root_cause: "redundant_organize_and_discarded_metadata"
resolution_type: "code_fix"
tags:
  - "catalog"
  - "sqlite"
  - "nfs-performance"
  - "incremental-cache"
  - "rayon"
---

# Eliminating Catalog Correction Script Hangs and Preserving Scanned Metadata

## Problem
When users clicked "Apply Corrections" or executed generated bash correction scripts from `catalog.html` on large archives or network mounts (e.g. NFS shares with ~1,000 photos), the script hung or took minutes to complete even for a single photo correction. The terminal output displayed:
```
== avicull correct ==
== avicull correct ==
Regenerating catalog with updated SQLite overrides...
== avicull organize ==
Discovered 987 total photo bundles in source.
```

## Symptoms
1. Executing a generated correction script from `catalog.html` took tens of seconds to minutes instead of milliseconds.
2. Terminal stalled during the organize phase over network mounts (`/Volumes/pictures/...`).
3. Running `avicull organize` on an existing dataset re-triggered neural inference across hundreds of unmodified photos instead of using cached decisions.

## Root Causes
Two compounding architectural bugs caused this behavior:

1. **Redundant `$AVICULL_CMD organize` in Catalog Shell Scripts**:
   `avicull correct` is already fully self-contained: it records the human override in `manifest.sqlite`, updates any in-place renames or hardlinks, and directly regenerates `catalog.html` in <0.2 seconds.
   However, `template.rs` had legacy scaffolding in `generateBatchScriptContent`, `buildMoveCommandsForItem`, and `buildReclassifyCommands` that blindly appended `$AVICULL_CMD organize --input "."`. This forced the entire pipeline to re-run: walking the directory tree over NFS, re-extracting EXIF, resolving gazetteer tokens, and evaluating neural classification.

2. **Discarded Metadata During Scans Falsely Triggering Reclassification**:
   In `src/scanner.rs`, `scan_input_tree_with_skip` naturally initializes new `AssetBundle` instances with `gps_coords: None`, `capture_timestamp_ms: None`, and `resolved_location_name: None`.
   When matching against `existing_bundle_map` in `src/main.rs`, only `original_stem` was copied from `old_bundle`. `gps_coords`, `capture_timestamp_ms`, and `resolved_location_name` were discarded.
   Consequently:
   - `resolve_locations` saw `bundle.gps_coords.is_none()` and sequentially re-read EXIF headers over the network mount.
   - When location tokens were assigned, `should_reclassify_bundle` checked `bundle.gps_coords != old_bundle.gps_coords`. Because `old_bundle.gps_coords` was `None` or mismatched, all 987 bundles were flagged as needing reclassification, triggering full neural network classification.

3. **Sequential EXIF Extraction**:
   `resolve_locations` in `src/geo.rs` iterated sequentially over bundles instead of using parallel threads, and re-attempted EXIF reads on bundles that already had timestamps.

## What Didn't Work
- Adding timeouts to `organize` or `nom-exif`: Did not fix the underlying cause that `organize` should never have been invoked for species corrections.
- Disabling gazetteer matching: Broke regional checklist priors and location naming.

## Solution
1. **Remove Trailing `organize` from Correction Scripts (`src/catalog/template.rs`)**:
   Removed `$AVICULL_CMD organize --input "."` from `buildMoveCommandsForItem`, `generateBatchScriptContent`, and `buildReclassifyCommands`. Generated scripts now finish in <0.2 seconds via `avicull correct`. Retained `organize` exclusively in location dialogs where reclassification against new regional priors is explicitly requested.
2. **Preserve Cached Metadata During Discovered Bundle Reconciliation (`src/main.rs`)**:
   When matching newly discovered bundles against `existing_bundle_map`, preserve `bundle.bundle_id`, `bundle.original_stem`, `bundle.gps_coords`, `bundle.capture_timestamp_ms`, and `bundle.resolved_location_name`. This ensures `should_reclassify_bundle` returns `false` for cached bundles.
3. **Rayon Parallelization and Timestamp Caching in `resolve_locations` (`src/geo.rs`)**:
   Switched `bundles.iter_mut()` to `bundles.par_iter_mut()` to parse EXIF in parallel. Added `if bundle.capture_timestamp_ms.is_none()` to completely bypass reading files on disk when metadata is already cached.
4. **Unified Catalog Regeneration (`src/organizer/mod.rs`)**:
   Extracted `regenerate_catalog_from_manifest` to deduplicate catalog construction across `correct`, `location`, and the `organize` fast-path, properly applying `bundle_locations` and `directory_locations` overrides.

## Prevention & Invariants
- **TDD Guard**: `test_batch_and_single_correction_scripts_do_not_contain_organize_call` in `tests/test_suite/test_catalog.rs` verifies that correction scripts do not contain `organize --input`.
- **Metadata Retention Guard**: `test_discovered_bundle_preserves_cached_metadata_from_existing_bundle_map` in `tests/test_suite/test_reclassify_unclassified_bundles.rs` guarantees cached bundles do not trigger reclassification.
