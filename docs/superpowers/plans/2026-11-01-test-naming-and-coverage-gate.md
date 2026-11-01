# Test Naming Harmonization & Build-Time Coverage Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harmonize test names, eliminate "inner loop / local user samples" ambiguity by re-centering around "mock directory structure", consolidate fragmented catalog tests into a coherent domain suite, systematically address module coverage gaps to push line coverage toward >95%, and install a build-time blocking coverage metric in pre-push hooks and CI.

**Architecture:** 
1. Re-center integration test and Playwright suite on mock directory structure.
2. Consolidate fragmented catalog HTML/DOM tests into `test_catalog_html_generation.rs` with uniform `test_<domain>_<scenario>_<expectation>` naming.
3. Add targeted unit/integration tests for `types.rs`, `geo.rs`, `classifier.rs`, `storage/network.rs`, and `update.rs`.
4. Install `cargo-llvm-cov` with a strict blocking threshold in `.githooks/pre-push` and `.github/workflows/pr_checks.yml`.

**Tech Stack:** Rust, Cargo, `cargo-llvm-cov`, Playwright TypeScript, bash git hooks, GitHub Actions.

---

### Task 1: Re-center Mock Directory Structure Tests
- [ ] Rename `tests/test_local_user_samples_e2e.rs` to `tests/test_mock_directory_structure_e2e.rs`
- [ ] Rename test function to `test_mock_directory_structure_full_organize_and_hardlink_integrity`
- [ ] Update `.githooks/pre-push` references
- [ ] Update `AGENTS.md` documentation
- [ ] Rename `tests/e2e/live_mock_catalog.spec.ts` to `tests/e2e/mock_directory_catalog.spec.ts`
- [ ] Verify both Rust test and Playwright test pass

### Task 2: Consolidate Fragmented Catalog HTML Integration Tests
- [ ] Consolidate `tests/test_catalog_card_display.rs`, `tests/test_bulk_location_modal.rs`, and `tests/test_reclassify_review_modal.rs` into `tests/test_catalog_html_generation.rs`
- [ ] Remove deleted files
- [ ] Standardize test function names with explicit domain semantics
- [ ] Verify `cargo test --test test_catalog_html_generation` passes

### Task 3: Address Coverage Gaps Across Core Modules
- [ ] Add tests in `tests/test_types.rs` for `ConfidenceMode`, `OrganizationMode` display, and all `AvicullError` display formatters
- [ ] Add tests in `tests/test_geo.rs` for DMS S/W, zero denom, companion EXIF fallback, and malformed TOML
- [ ] Add tests in `tests/test_classifier.rs` for `compute_posterior` edge cases, empty/unknown regional prior multipliers, and `OrtSpeciesClassifier`
- [ ] Add tests in `tests/test_storage_backend.rs` for `NetworkShareBackend` cache hit, retry, unmapped eviction, and default constructor
- [ ] Add tests in `tests/test_update.rs` for notice cache expiry, checksum verification errors, and atomic replace cleanup
- [ ] Run `cargo llvm-cov` to confirm significant line coverage increase across all modules

### Task 4: Add Build-Time Blocking Coverage Gate
- [ ] Add coverage check step in `.githooks/pre-push` with threshold gate
- [ ] Add coverage check job in `.github/workflows/pr_checks.yml` using `taiki-e/install-action@cargo-llvm-cov`
- [ ] Run pre-push hook locally to verify it blocks if coverage drops and passes cleanly
- [ ] Commit and verify on GitHub Actions
