# Task 2 Report: Re-evaluation of Mock-Failed and Location-Updated Bundles

## Commit
- SHA: `2d5a86f`
- Subject: `feat(organize): re-evaluate unclassified mock-failed and location-updated bundles`

## Changes Summary
1. **CLI Flag (`--reclassify`)**:
   - Added `reclassify: bool` flag to `OrganizeArgs` in `src/cli.rs`.
   - Allows users to explicitly force re-evaluation of non-override cached photo bundles without losing manifest history or overrides.
2. **Reclassification Logic (`should_reclassify_bundle`)**:
   - Implemented in `src/organizer/mod.rs` with strict invariant precedence:
     1. User overrides (`existing_decision.is_user_override == true`) are permanent and inviolable (always `false`).
     2. Explicit `--reclassify` flag forces re-classification (`true`).
     3. Spatial & coordinate updates (`bundle.gps_coords != old_bundle.gps_coords`) trigger Bayesian geographic prior re-evaluation (`true`).
     4. Untouched bundles retain cached decisions (`false`), preserving 100% cache idempotency and cross-machine portability.
3. **Organize Pipeline Integration (`src/main.rs`)**:
   - Updated the cache partitioning loop in `src/main.rs` to evaluate each cached bundle via `should_reclassify_bundle`.
   - Added helpful user tip when cached items with no bird detected are present, advising on `--reclassify`.
   - Deferred model initialization until after partitioning so cached no-op runs remain lightning-fast.
4. **Comprehensive Unit & E2E Tests (`tests/test_suite/test_reclassify_unclassified_bundles.rs`)**:
   - 9 test cases covering:
     * User override preservation with `--reclassify`.
     * User override preservation with updated coordinates.
     * Reclassification on normal decisions with `--reclassify`.
     * Cache retention for mock-failed decisions without `--reclassify`.
     * Reclassification of mock-failed decisions with `--reclassify`.
     * Reclassification on newly resolved coordinates.
     * Reclassification on changed coordinates across different locations.
     * Cache retention for untouched bundles with matching coordinates.
     * CLI argument parsing for `--reclassify`.
     * End-to-end manifest DB coordinate update and reclassification.

## Verification Results
- `cargo check --all-targets`: Passed with 0 errors.
- `cargo fmt --check`: Passed with 0 differences.
- `cargo clippy --all-targets --all-features -- -D warnings`: Passed with 0 warnings.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: Passed with 0 warnings.
- `cargo test --test test_suite`: 340/340 tests passed.
- `cargo test --test test_mock_directory_structure_e2e`: Passed.
