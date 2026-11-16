---
title: "Resolving Companion Preview Paths in Nested Subdirectories and Remedying Cloud Eviction"
date: "2026-11-16"
category: "logic-errors"
module: "catalog"
problem_type: "logic_error"
component: "catalog_and_preview_generator"
symptoms:
  - "Photos in nested subdirectories fail to render previews in single-trip catalogs and display false-positive Cloud Evicted fallback cards"
  - "Browser developer console logs 404 errors attempting to load previews from nonexistent subdirectory paths"
  - "Users cannot distinguish between genuinely evicted cloud files (Darwin SF_DATALESS) and missing disk previews"
root_cause: "logic_error"
resolution_type: "code_fix"
severity: "high"
tags:
  - "catalog"
  - "previews"
  - "subdirectories"
  - "fileprovider"
  - "dataless"
  - "macos"
---

# Resolving Companion Preview Paths in Nested Subdirectories and Remedying Cloud Eviction

## Problem
In photographic archives containing nested subdirectories (such as lens trials or multi-session folders within a single trip), `catalog.html` failed to display precomputed preview thumbnails when using decoupled view projection (`--reclassify-mode fast` or `ThumbnailEmbeddingPolicy::Suppressed`). The browser console reported 404 errors, and cards fell back to a generic warning claiming the images were cloud-evicted or unsupported RAW formats, even when all preview JPEGs were locally resident on disk in `.avicull/previews/`. Furthermore, for genuinely cloud-evicted files, users had no mechanism to discover which files were missing or to trigger hydration without inspecting raw filesystem attributes in Terminal.

## Symptoms
- Opening `catalog.html` on a trip archive containing subdirectories (e.g. `pictures_seattle-2017-2019/2470 trial/`) displayed fallback cards with the message `"Preview Unavailable (Cloud Evicted or Unsupported RAW)"` for all photos in subdirectories.
- Inspecting the local archive revealed that all preview JPEGs (e.g. 3,721 files) were intact and resident on disk in `/Volumes/pictures/pictures_seattle-2017-2019/.avicull/previews/`.
- Browser network inspection logged 404 Not Found errors trying to load `2470 trial/avicull_catalog_files/previews/{stem}.jpg`.
- When files were genuinely evicted by macOS FileProvider (Darwin `SF_DATALESS`), `catalog.html` provided no indication of which specific files were dataless or how to download them.

## What Didn't Work
- **Unconditionally prepending the relative subdirectory to companion paths**: `build_catalog_payload_item` previously formatted preview paths by prepending `{subdirectory}` (`format!("{sub}/{CATALOG_FILES_DIR}/previews/{stem}.jpg")`). In single-trip archives, companion preview files reside strictly at the trip root (`<input>/avicull_catalog_files/previews/` or `<input>/.avicull/previews/`), never nested inside image subdirectories.
- **Relying on browser JavaScript for automatic background hydration**: Web browsers execute inside a strict security sandbox on `file:///` URLs and cannot execute arbitrary host shell commands or communicate with Darwin kernel FileProvider daemons without external daemon bridges.
- **Generic `img.onerror` catch-all handling**: Treating every image load failure as a cloud eviction produced 100% false-positive eviction notices on path resolution bugs or missing preview files.

## Solution

### 1. Hierarchical Preview Path Resolution (`src/catalog/mod.rs`)
Implemented `resolve_relative_preview_path` to deterministically resolve preview paths across both single-trip archives (with or without subdirectories) and parent rollup catalogs:

```rust
// src/catalog/mod.rs
pub fn resolve_relative_preview_path(
    output_root: &Path,
    stem: &str,
    subdirectory: Option<&str>,
    existing_previews: Option<&HashSet<String>>,
) -> String {
    let unhidden_filename = format!("{CATALOG_FILES_DIR}/previews/{stem}.jpg");
    let legacy_filename = format!("{LEGACY_CATALOG_FILES_DIR}/previews/{stem}.jpg");

    // Tier 1: Check root level first (single-trip archives always store previews at root)
    if let Some(existing) = existing_previews {
        if existing.contains(&unhidden_filename) {
            return unhidden_filename;
        }
        if existing.contains(&legacy_filename) {
            return legacy_filename;
        }
    } else {
        if output_root.join(&unhidden_filename).is_file() {
            return unhidden_filename;
        }
        if output_root.join(&legacy_filename).is_file() {
            return legacy_filename;
        }
    }

    // Tier 2: Check child directory prefixes for parent rollup catalogs
    if let Some(sub) = subdirectory {
        let trimmed = sub.trim_matches('/');
        let top_sub = trimmed.split('/').next().unwrap_or(trimmed);
        let candidate_dirs = if top_sub == trimmed {
            vec![trimmed]
        } else {
            vec![trimmed, top_sub]
        };

        for candidate in candidate_dirs {
            let candidate_unhidden = format!("{candidate}/{CATALOG_FILES_DIR}/previews/{stem}.jpg");
            let candidate_legacy = format!("{candidate}/{LEGACY_CATALOG_FILES_DIR}/previews/{stem}.jpg");

            if let Some(existing) = existing_previews {
                if existing.contains(&candidate_unhidden) {
                    return candidate_unhidden;
                }
                if existing.contains(&candidate_legacy) {
                    return candidate_legacy;
                }
            } else {
                if output_root.join(&candidate_unhidden).is_file() {
                    return candidate_unhidden;
                }
                if output_root.join(&candidate_legacy).is_file() {
                    return candidate_legacy;
                }
            }
        }
    }

    // Tier 3: Deterministic fallback preferring legacy if present
    if output_root.join(LEGACY_CATALOG_FILES_DIR).is_dir() {
        legacy_filename
    } else {
        unhidden_filename
    }
}
```

### 2. Client-Side Eviction Discrimination and Remediation (`src/catalog/template.rs`)
1. **Locality Verification**: `handleFinalImageFallback` inspects `item.is_local` (populated by `avicull` scanning Darwin `SF_DATALESS` `0x40000000`). If `is_local === true`, the fallback card clearly reports `"Preview Missing on Disk"` and offers a `📋 Copy Render Command` button. If `is_local === false`, it reports `"Cloud Evicted (Dataless File)"` and offers a `📋 Copy Download Command` button.
2. **Hydration Command via FileProvider**: The download command uses `head -c 1 "$filepath" > /dev/null`, which prompts macOS Darwin FileProvider to hydrate the dataless file on-demand.
3. **Evicted Files Helper Modal**: An amber `☁️ Evicted Files (N)` badge button in the header toolbar opens a dialog listing all dataless files, offering 1-click batch copy commands and a downloadable `materialize_evicted.sh` executable script.

## Why This Works
In single-trip archives, photographic files can be organized in arbitrary nested folders, but the manifest and companion previews are stored strictly at the trip root (`avicull_catalog_files/previews/`). In parent rollup catalogs, each child trip maintains its own companion preview folder (`{child_trip}/avicull_catalog_files/previews/`). By prioritizing the trip root first, `resolve_relative_preview_path` ensures single-trip photos resolve cleanly without invalid subdirectory path prefixes, while still resolving child-trip paths in multi-trip parent rollups.

Inspecting `is_local` on the client eliminates false-positive eviction notices, and generating Darwin FileProvider hydration scripts empowers users to materialize evicted files directly without complex manual filesystem queries.

## Prevention
- **Unit and Integration Tests**: Added regression tests in `tests/test_suite/test_dual_thumbnails.rs`:
  - `test_single_trip_nested_subdirectory_resolves_legacy_root_previews`
  - `test_single_trip_nested_subdirectory_resolves_unhidden_root_previews`
  - `test_parent_rollup_nested_child_subdirectory_resolves_child_root_previews`
- **Visual E2E Regression Testing**: Automated Playwright visual snapshots in `tests/e2e/catalog_visual_snapshot.spec.ts` verify the fallback card layout and action buttons.
- **Coding Standard**: Mandated in `CODING_STANDARDS.md` under `Client-Side Catalog UI & Network Mount Responsiveness` that relative preview paths must never blindly prepend image subdirectories, and client-side UI must verify `item.is_local` before declaring assets cloud-evicted.

## Related Issues
- **PR #68**: Fix companion preview paths in nested subdirectories and add evicted files download helper
- **PR #11**: Space-efficient cloud and network mount support
- **Related Docs**:
  - [macos-fileprovider-dataless-hydration-prevention.md](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/solutions/runtime-errors/macos-fileprovider-dataless-hydration-prevention.md)
  - [remote-nas-parent-rollup-and-view-decoupling.md](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/solutions/performance-issues/remote-nas-parent-rollup-and-view-decoupling.md)
