---
title: "Standalone Inference Script Resolution and Reclassification Precedence"
date: "2026-11-11"
category: "logic-errors"
module: "organizer,classifier"
problem_type: "logic_error"
component: "service_layer"
severity: "high"
supersedes: "standalone-inference-resolution-and-location-reclassification.md"
symptoms:
  - "Running organize on external/NFS photo archives classifies all bundles as _no_bird_detected"
  - "Updating directory coordinates in catalog.html does not trigger species reclassification on re-scan"
  - "Incremental organize runs silently fall back to MockBirdDetector when executed outside repository root"
  - "Passing --reclassify skips or restores human-overridden bundles instead of re-evaluating the entire archive"
root_cause: "logic_error"
resolution_type: "code_fix"
tags:
  - "standalone-inference"
  - "embedded-assets"
  - "reclassification"
  - "spatial-priors"
  - "nfs-mount"
  - "manifest-caching"
  - "user-override"
---

# Standalone Inference Script Resolution and Reclassification Precedence

## Problem
When executing `avicull organize --input <PATH>` on photo archives outside the repository workspace (such as NFS mounts, external drives, or system-installed binaries):
1. `avicull` failed to locate `scripts/infer.py` when probed relative to the current working directory, falling back to `MockBirdDetector` / `MockSpeciesClassifier` and routing photos to `_no_bird_detected`.
2. `models_dir` lacked `regional_checklists.json` in standalone cache environments, disabling regional checklist Bayesian priors.
3. Updating capture coordinates for a folder or bundle (e.g., via `catalog.html` or `avicull location`) left existing manifest bundles skipped during incremental re-runs instead of re-evaluating them under the updated spatial priors.
4. An earlier reclassification order checked `existing_decision.is_user_override` ahead of `--reclassify` and unconditionally re-applied overrides after burst consensus, preventing `--reclassify` from performing a full from-scratch archive re-evaluation when explicitly requested.

## Symptoms
- Bird photo archives located outside the repository root classify entirely into `_no_bird_detected`, except subdirectories previously processed from within the repository workspace.
- Fallback logs indicate real inference could not run (`scripts/infer.py` missing) and mock classifiers are used.
- Updating directory coordinates via `catalog.html` followed by `avicull organize` completes in milliseconds without reclassifying affected bundles.
- Running `avicull organize --reclassify` preserves or restores previous human overrides instead of re-running neural detection and classification across the entire archive.

## What Didn't Work
- **Naive auto-reclassification of zero-confidence assets**: Re-inferring any cached bundle with `detection_confidence == 0.0 && top_predictions.is_empty()` broke incremental caching. Genuine non-bird and landscape photographs legitimately produce zero avian confidence, causing every subsequent `organize` run to re-run neural inference across all non-bird frames.
- **Working-directory-only script lookup**: Checking only `./scripts/infer.py` succeeds only inside the repository root and fails whenever the binary is installed globally or invoked from external archive directories.
- **Checking `is_user_override` before `force_reclassify` and unconditionally restoring overrides post-burst**: Returning `false` on `existing_decision.is_user_override` ahead of `force_reclassify` in `should_reclassify_bundle` (combined with unconditional post-`smooth_burst_consensus` override restoration in `src/main.rs`) left users with no CLI path to force a clean from-scratch archive reclassification via `--reclassify`.

## Solution
The fix combines self-contained embedded asset extraction with an explicit four-tier reclassification precedence model and `!args.reclassify`-guarded burst override preservation:

1. **Self-Contained Embedded Asset Resolution (`src/classifier.rs`, `src/main.rs`)**:
   - `resolve_inference_script` / `resolve_inference_script_in` (`src/classifier.rs:50-115`) embeds `EMBEDDED_INFERENCE_SCRIPT` (`include_str!("../scripts/infer.py")`, line 36) and resolves `infer.py` in priority order:
     1. `./scripts/infer.py` relative to the current working directory.
     2. `<current_exe_dir>/scripts/infer.py` adjacent to the executable.
     3. Cache extraction to `$AVICULL_CACHE_DIR/scripts/infer.py` or `~/.cache/avicull/scripts/infer.py` (falling back to `<temp_dir>/avicull/scripts/infer.py` if `HOME` is unset), atomically writing via a temp file with Unix mode `0o755` whenever missing or when the `blake3` digest differs from `EMBEDDED_INFERENCE_SCRIPT`.
   - `sync_regional_checklists` (`src/classifier.rs:128-146`) embeds `EMBEDDED_REGIONAL_CHECKLISTS_JSON` (`include_str!("../data/regional_checklists.json")`, lines 39-40) and atomically writes `regional_checklists.json` under the target models directory whenever missing or empty while leaving valid custom checklists untouched. `resolve_or_prompt_models_dir` invokes it at `src/main.rs:488-490`, and `run_batch_inference` resolves the script at `src/main.rs:154`.

2. **Four-Tier Reclassification Precedence (`src/organizer/mod.rs:647-663`)**:
   During bundle partitioning (`src/main.rs:863-898`), `should_reclassify_bundle` evaluates cached bundles in strict priority order:

```rust
#[must_use]
pub fn should_reclassify_bundle(
    bundle: &AssetBundle,
    existing_decision: &BundleDecision,
    old_bundle: &AssetBundle,
    force_reclassify: bool,
) -> bool {
    if force_reclassify {
        return true;
    }
    if existing_decision.is_user_override {
        return false;
    }
    if bundle.gps_coords != old_bundle.gps_coords {
        return true;
    }
    false
}
```

   Before this check runs, `src/main.rs:632-646` transfers cached `gps_coords`, `capture_timestamp_ms`, and `resolved_location_name` onto newly scanned bundles when scanner defaults are `None`, preventing unchanged cached bundles from triggering false coordinate deltas.

3. **Burst-Consensus Override Guard & `--reclassify` CLI Contract (`src/main.rs`, `src/cli.rs`, `src/help.rs`)**:
   - After `smooth_burst_consensus`, `src/main.rs:1196-1209` re-applies `existing_dec.is_user_override` decisions only when `!args.reclassify`. Normal incremental `organize` runs keep manual corrections intact across burst smoothing (even if adjacent burst frames change), while `--reclassify` clears and re-evaluates all bundles from scratch.
   - Updated `--reclassify` documentation in `src/cli.rs:214-216` and `src/help.rs:105-108, 149-157, 186-190, 276`.

## Why This Works
- **Location-independent execution**: Embedding and hash-syncing `infer.py` alongside `regional_checklists.json` guarantees identical detection, classification, and regional checklist priors regardless of working directory or install path, while still preferring local `./scripts/infer.py` during development.
- **Balanced incremental caching vs. override control**: Placing `force_reclassify` at tier 1 and `is_user_override` at tier 2 (backed by the `!args.reclassify` burst-consensus guard) ensures normal incremental runs preserve human edits and skip unchanged bundles—including genuine zero-confidence non-bird frames—while coordinate updates still re-evaluate non-overridden assets and `--reclassify` remains an unambiguous full-archive reset.

## Prevention
- Follows the invariants codified in `CODING_STANDARDS.md:70-76` under `Standalone Distribution & Incremental Manifest Invariants` (`Self-Contained Executable Asset Resolution` and `Reclassification Precedence & Incremental Cache Protection`).
- Covered by consolidated integration tests:
  - `tests/test_suite/test_standalone_inference_resolution.rs` (lines 1-202): verifies embedded assets, CWD/exe/cache fallback order, `blake3` stale cache replacement, and `sync_regional_checklists`.
  - `tests/test_suite/test_reclassify_unclassified_bundles.rs` (lines 1-379): verifies `--reclassify` > `is_user_override` > `gps_coords` precedence, cached metadata transfer, CLI parsing, and SQLite manifest persistence.
  - `tests/test_suite/test_human_override_preservation.rs` (lines 29-148): executes the compiled binary end-to-end to verify normal `organize` preserves SQLite overrides and `catalog.html` labels while `organize --reclassify` resets and reclassifies from scratch.

## Related Issues
- Initial standalone inference resolution and coordinate-delta reclassification merged in PR `#55`; precedence inversion and `--reclassify` override reset merged in PR `#58`.
- Relates to `docs/solutions/logic-errors/bayesian-spatial-temporal-prior-fusion.md`, `docs/solutions/architecture-patterns/inplace-cataloging-and-cosmetic-rename-decoupling.md`, and `docs/solutions/performance-issues/catalog-correction-script-hang-and-cached-metadata-retention.md`.
