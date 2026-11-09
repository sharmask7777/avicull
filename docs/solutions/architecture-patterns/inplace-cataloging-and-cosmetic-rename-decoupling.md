---
title: "In-Place Non-Destructive Cataloging and Cosmetic File Renaming Decoupling"
date: "2026-11-09"
category: "architecture-patterns"
module: "organizer"
problem_type: "architecture_pattern"
component: "core"
severity: "critical"
applies_when:
  - "Designing default non-destructive photo cataloging workflows"
  - "Decoupling SQLite metadata tracking from physical on-disk file renaming"
  - "Handling materialization mode transitions on cached manifest databases"
  - "Generating client-side catalog scripts that respect source immutability"
tags:
  - "source-immutability"
  - "in-place"
  - "cosmetic-renaming"
  - "manifest-caching"
  - "catalog-generation"
---

# In-Place Non-Destructive Cataloging and Cosmetic File Renaming Decoupling

## Context
Photo archives often contain thousands of RAW images, companion JPEGs, and auxiliary adjustment sidecars (`.xmp`, `.dop`). Photographers require automated classification, burst consensus pooling, and interactive gallery exploration without altering original camera filenames (`DSC_0001.JPG`) that are referenced in digital asset management (DAM) catalogs or external backup manifests.

Historically, in-place tools conflated cataloging with descriptive file renaming, renaming source assets directly on disk. This risked breaking external file references and compromised strict source immutability.

## Guidance

### 1. Dual In-Place Architecture: Non-Destructive Default vs Opt-In Cosmetic Rename
`avicull` decouples in-place operations into two distinct modes:

1. **Default In-Place Mode (`--mode in-place`)**:
   - The input directory (`--input`) is strictly read-only for photographic assets: never move, rename, delete, or modify any source file, companion, or sidecar.
   - All state is entirely self-contained within `<input>/.avicull/manifest.sqlite` (decisions, embeddings, run records) and `<input>/catalog.html`.
   - File reconciliation (`reconcile_inplace_manual_renames`) and client-side fallback scripts in `catalog.html` must **never** issue filesystem rename calls (`fs::rename` or bash `mv`) in this mode.

2. **Cosmetic Renaming Mode (`--mode in-place-rename`)**:
   - Explicitly requested opt-in for users who desire descriptive filenames on disk (`<stem>_<Category>_<Species>[_<Date>][_<Location>].<ext>`).
   - Renames primary images, companion JPEGs, and all associated sidecars in atomic lockstep.
   - Every rename pair is transactionally recorded in `.avicull/manifest.sqlite` under `operation_type = "in_place_rename"` with `status = "renamed"` for lossless rollback via `avicull clean`.

### 2. Mode Transitions & Neural Cache Reuse
When users re-run `organize` with a different materialization mode (e.g. running default `in-place` first, then switching to `in-place-rename` or `hierarchy`):
- The pipeline queries `db.get_latest_run_mode()`. If the mode changed, it bypasses the early return while reusing all cached classification decisions from `manifest.sqlite`.
- Model directory prompts and neural inference backbones are skipped when `new_bundles.is_empty()`, materializing the new mode in milliseconds.
- Visual embeddings in SQLite must be protected against nullification during re-runs by employing `COALESCE(excluded.visual_embedding, bundle_decisions.visual_embedding)` in SQLite conflict update statements.

### 3. Active Path Resolution in Catalog Regeneration
When human corrections (`avicull correct`) are applied in `InPlaceRename` mode:
- The catalog generator must resolve active on-disk paths from `db.get_bundle_operations(&bundle_id)` (`op.operation_type == "in_place_rename"` and `op.dest_path.exists()`), rather than referencing stale pre-rename stems from initial bundle discovery.

## Why This Matters
- **Safety by Default**: Users can run `avicull organize` on their personal photo library with zero risk of file mutation.
- **Flexibility**: Users who want descriptive filenames on disk can opt in with `--mode in-place-rename`, knowing all changes are tracked and reversible.
- **Zero Redundant Inference**: Switching between catalog views, rename modes, or hardlink hierarchies consumes negligible CPU/GPU time by leveraging persistent SQLite manifests.

## Examples
Verified across the test harness:
- `tests/test_suite/test_inplace_no_rename.rs`: Asserts byte-level and inode immutability of source assets in default `in-place` mode.
- `tests/test_suite/test_cli_correct.rs`: Validates metadata-only updates in `in-place` mode and active renamed path resolution in `in-place-rename` mode.
- `tests/test_suite/test_catalog_html_generation.rs`: Verifies that fallback client-side script generators in `catalog.html` never emit `mv` commands in default `in-place` mode.
