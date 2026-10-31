# Implementation Plan: Catalog Snapshot & Visual Regression Testing

## Problem Statement
We want to store a canonical baseline snapshot of `bird_catalog.html` generated from the committed mini-dataset (`tests/fixtures/local_user_samples`), and establish two automated regression guardrails:
1. **Raw HTML Snapshot Testing**: Test that the rendered HTML output matches the canonical structure, elements, scripts, CSS classes, and metadata without unexpected text or DOM attribute regressions.
2. **Visual Layout Snapshot Testing (Playwright)**: Capture visual screenshot snapshots (`toHaveScreenshot`) in Playwright across key states (e.g. initial render, modal views) to catch layout drift, css regressions, or misalignments.

---

## Proposed Architecture

1. **Canonical HTML Snapshot**:
   - Location: `tests/fixtures/snapshots/bird_catalog_reference.html` (normalized for dynamic run timestamps if any, or generated from deterministic fixture data).
   - Test: `tests/test_catalog_snapshot.rs` (Rust integration test).
     - Renders catalog from deterministic test items / committed mock dataset.
     - Compares against baseline or verifies critical DOM structure and script parity.

2. **Visual Playwright Snapshot Testing**:
   - Location: `tests/e2e/catalog_visual_snapshot.spec.ts`
   - Playwright's native `expect(page).toHaveScreenshot('catalog-main-layout.png')` with controlled viewport (`1280x800`), font rendering, and animations disabled.
   - Snapshots directory: `tests/e2e/catalog_visual_snapshot.spec.ts-snapshots/` committed to repository.

3. **Pre-push & CI Integration**:
   - Integrate the visual snapshot check into Playwright test suite (`npm test`).
   - Integrate `cargo test --test test_catalog_snapshot` into pre-push test run.

---

## Tasks

### Task 1: Rust Raw HTML Snapshot Integration Test
- [ ] Create `tests/test_catalog_snapshot.rs`
- [ ] Generate catalog from the committed mock dataset
- [ ] Establish reference snapshot in `tests/fixtures/snapshots/`
- [ ] Normalize nondeterministic fields (e.g. execution timestamps / dates) if any
- [ ] Verify test passes and asserts structural and content parity

### Task 2: Playwright Visual Regression Snapshot Test
- [ ] Create `tests/e2e/catalog_visual_snapshot.spec.ts`
- [ ] Render mock catalog page
- [ ] Capture baseline screenshots for main catalog, location modal, and reclassify review modal using `toHaveScreenshot()`
- [ ] Run `npx playwright test catalog_visual_snapshot.spec.ts --update-snapshots`
- [ ] Verify `npx playwright test` passes cleanly with 0 diff

### Task 3: Multi-Agent Crew Audit & Push
- [ ] Dispatch `correctness-safety-reviewer`
- [ ] Dispatch `rust-readability-reviewer`
- [ ] Dispatch `spec-conformance-reviewer`
- [ ] Run full `.githooks/pre-push` verification suite
- [ ] Create PR
