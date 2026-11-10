---
title: "Resolving Standalone Inference Script Fallbacks and Location-Updated Reclassification"
date: "2026-11-10"
category: "logic-errors"
module: "organizer"
problem_type: "logic_error"
component: "service_layer"
severity: "high"
symptoms:
  - "Running organize on external/NFS photo archives classifies all bundles as _no_bird_detected"
  - "Updating directory coordinates in catalog.html does not trigger species reclassification on re-scan"
  - "Incremental organize runs silently falling back to MockBirdDetector when executed outside repository root"
root_cause: "logic_error"
resolution_type: "code_fix"
tags:
  - "standalone-inference"
  - "embedded-assets"
  - "reclassification"
  - "spatial-priors"
  - "nfs-mount"
  - "manifest-caching"
---

# Resolving Standalone Inference Script Fallbacks and Location-Updated Reclassification

## Problem
When executing `avicull organize --input <PATH>` on photo archives located outside the repository workspace (such as on network-attached storage or external disks):
1. All photo bundles are assigned to `_no_bird_detected` because the binary fails to find `scripts/infer.py` relative to the current working directory, silently falling back to a stub mock detector.
2. The regional species checklist database (`regional_checklists.json`) is absent from the external environment's cache directory, preventing spatial-temporal Bayesian prior adjustments.
3. When capture coordinates are subsequently updated for a directory (via `catalog.html`), re-running `avicull organize` skips all existing bundles without re-evaluating their classifications against the newly assigned location.

## Symptoms
- An archive of hundreds of bird photos is categorized entirely under `_no_bird_detected`, except for subdirectories previously classified inside the repository workspace.
- Command-line logs output warnings indicating `scripts/infer.py` was not found and `MockBirdDetector` is active.
- Modifying folder GPS coordinates via the web catalog and re-running `avicull organize` finishes in milliseconds with "0 new bundles to classify", leaving classifications unchanged.

## What Didn't Work
- **Naive auto-reclassification of zero-confidence assets**: Automatically reclassifying any bundle where `detection_confidence == 0.0 && top_predictions.is_empty()` broke incremental caching. Because non-bird and landscape photos legitimately produce zero avian confidence, this caused repeat runs to indefinitely re-infer all non-bird images, destroying sub-second incremental execution.
- **Relying solely on working-directory relative paths**: Checking `Path::new("scripts/infer.py")` works only when `avicull` is executed directly from the project root. When installed to `/usr/local/bin` or invoked from external mount points, relative path probes fail.

## Solution
The resolution implements self-contained asset extraction and a strict four-tier reclassification precedence model:

1. **Self-Contained Embedded Asset Extraction**:
In `src/main.rs` and `src/classifier.rs`, `resolve_inference_script` and checklist discovery use a hierarchical fallback ladder:
- Check current working directory (`scripts/infer.py` and `data/regional_checklists.json`).
- Check binary-adjacent directories (`std::env::current_exe()`).
- Unpack embedded compile-time assets (`include_str!("../scripts/infer.py")` and `include_str!("../data/regional_checklists.json")`) into `~/.cache/avicull/scripts/` and `~/.cache/avicull/models/`.

2. **Explicit Reclassification Flag (`--reclassify`)**:
Added `--reclassify` to `src/cli.rs` and CLI argument parsing, allowing users to explicitly force re-evaluation of an entire archive when models or taxonomies are updated.

3. **Four-Tier Reclassification Precedence**:
In `src/organizer/mod.rs`, `should_reclassify_bundle` evaluates previously scanned bundles:

```rust
pub fn should_reclassify_bundle(
    bundle: &AssetBundle,
    existing_decision: &BundleDecision,
    old_bundle: &AssetBundle,
    force_reclassify: bool,
) -> bool {
    // Tier 1: Human corrections are inviolable
    if existing_decision.is_user_override {
        return false;
    }
    // Tier 2: Explicit user CLI flag
    if force_reclassify {
        return true;
    }
    // Tier 3: Location / coordinate delta
    if bundle.gps_coords != old_bundle.gps_coords {
        return true;
    }
    // Tier 4: Idempotent manifest cache retention
    false
}
```

## Why This Works
- Self-contained asset resolution ensures the compiled binary functions autonomously regardless of execution directory, system installation path, or external mount points.
- The four-tier precedence model ensures that location changes (such as directory overrides applied in `catalog.html`) immediately trigger re-classification against regional checklists, while preserving user corrections and avoiding redundant neural inference on unchanged assets.

## Prevention
- Added integration tests in `tests/test_suite/test_standalone_inference_resolution.rs` verifying fallback ladder resolution and embedded asset extraction.
- Added integration tests in `tests/test_suite/test_reclassify_unclassified_bundles.rs` verifying GPS coordinate change triggering, `--reclassify` flag behavior, and user correction immutability.
- Added strict invariants to `CODING_STANDARDS.md` under `## Standalone Distribution & Incremental Manifest Invariants`:
  - `Self-Contained Executable Asset Resolution`
  - `Reclassification Precedence & Incremental Cache Protection`

## Related Issues
- Solved in branch `fix/standalone-inference-and-reclassification`.
- Relates to `docs/solutions/logic-errors/bayesian-spatial-temporal-prior-fusion.md` (regional checklists and GPS priors) and `docs/solutions/architecture-patterns/inplace-cataloging-and-cosmetic-rename-decoupling.md` (manifest caching and incremental runs).
