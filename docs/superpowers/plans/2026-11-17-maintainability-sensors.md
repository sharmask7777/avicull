# Maintainability Sensors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement automated maintainability sensors across Rust, Python, and TypeScript to prevent code degradation, monoliths, architectural coupling, and silent regressions, complete with self-correction guidance and E2E smoke integration.

**Architecture:** Add deterministic static sensors (Ruff complexity rules, Clippy cognitive complexity), structural architectural boundaries (layer contracts in Rust and Python), targeted mutation testing scripts, and enriched feedback wrappers inside canonical verification scripts (`verify.sh`, `verify_python.sh`).

**Tech Stack:** Rust (Clippy, AST test parser, `cargo-mutants`), Python (Ruff, Mypy, `import-linter`, Pytest), TypeScript/Node (Playwright), Bash.

**Spec:** `docs/superpowers/specs/2026-11-17-maintainability-sensors-design.md`

## Global Constraints
- **Strict Source Immutability**: Verification and test steps must never modify image trees or test fixtures.
- **Apple Silicon Core & Memory Protection**: Honor `CARGO_BUILD_JOBS` and `RUST_TEST_THREADS=4` to keep system responsive.
- **Offline Capable**: Pipeline and tools must execute cleanly offline (`UV_EXTRA_FLAG=--offline`, local Playwright browsers).
- **Graceful Tool Degradation**: If optional developer tools (`cargo-deny`, `cargo-mutants`, `npm`) are absent, print friendly yellow skip messages rather than crashing.

## Review Focus
1. **Clippy Cognitive Complexity False Positives**: Embedded 3,000-line HTML generator `src/catalog/template.rs` must be explicitly exempted via file-level attribute to avoid breaking build gates.
2. **Monolithic Function Refactoring in `scripts/infer.py`**: Refactoring `process_image` into helper methods must not alter the output dict structure or detection confidence keys expected by downstream callers.
3. **Playwright Execution Latency**: E2E integration in `verify.sh` must only run a quick smoke spec (`mock_directory_catalog.spec.ts`) with a 15s timeout, not the full test suite.
4. **Rust Architecture Contract Edge Cases**: AST module scanning must correctly identify allowed dependencies without false positives on internal test modules (`cfg(test)`).
5. **Deterministic Mutation Testing**: Mutation script must default to isolated file targets (`src/types.rs`, `src/burst.rs`) and not trigger full-crate rebuild loops.

---

### Task 1: Python Complexity Rules & Refactoring Monolithic Helpers

**Files:**
- Modify: `pyproject.toml:17-38`
- Modify: `scripts/infer.py:587-730`
- Test: `scripts/verify_python.sh`

**Interfaces:**
- Consumes: `scripts/infer.py:CropBoxDetector.process_image`
- Produces: Cleaner, modular Python inference pipeline satisfying `C901` (complexity <= 15), `PLR0912` (branches <= 12), `PLR0913` (args <= 6), `PLR0915` (statements <= 50).

- [ ] **Step 1: Write the failing verification check in `pyproject.toml`**
Enable `C901`, `PLR0912`, `PLR0913`, and `PLR0915` in `pyproject.toml` under `tool.ruff.lint.select`.

- [ ] **Step 2: Run verification to confirm the failure**
Run: `uv run --with ruff ruff check scripts/`
Expected: FAIL with `C901`, `PLR0912`, `PLR0913`, `PLR0915` in `scripts/infer.py`.

- [ ] **Step 3: Refactor `process_image` in `scripts/infer.py`**
Decompose `process_image` into private modular helper methods:
- `_prepare_crop(...)`
- `_run_detection_pass(...)`
- `_filter_and_format_detections(...)`
Ensure `process_image` has complexity <= 10 and statements <= 40 while preserving exact signature and return format.

- [ ] **Step 4: Run Ruff and Python test suite to verify it passes**
Run: `./scripts/verify_python.sh`
Expected: PASS with 100% ruff, mypy, and pytest coverage >= 80%.

- [ ] **Step 5: Commit**
```bash
git add pyproject.toml scripts/infer.py
git commit -m "feat(sensors): enforce Python complexity rules and refactor infer.py"
```

---

### Task 2: Rust Cognitive Complexity Sensors & Hotspot Refactoring

**Files:**
- Modify: `Cargo.toml:47-70`
- Modify: `src/burst.rs:106-250`
- Modify: `src/catalog/template.rs:1-10`
- Test: `tests/test_suite/test_burst_detector.rs`

**Interfaces:**
- Consumes: `src/burst.rs::smooth_burst_consensus`
- Produces: Clippy cognitive complexity gate (< 30) across all Rust source modules.

- [ ] **Step 1: Enable `cognitive_complexity` in `Cargo.toml` and template exemption**
Add `cognitive_complexity = { level = "deny", priority = -1 }` with threshold 30 to `[lints.clippy]`. Add `#![allow(clippy::cognitive_complexity)]` with documentation to `src/catalog/template.rs`.

- [ ] **Step 2: Run cargo clippy to observe failing hotspots**
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: FAIL on `src/burst.rs::smooth_burst_consensus` (complexity 27-31).

- [ ] **Step 3: Refactor `smooth_burst_consensus` in `src/burst.rs`**
Extract cluster grouping and vote aggregation into focused helper functions:
- `fn group_items_by_window(...) -> Vec<Vec<&ClassifiedBundleItem>>`
- `fn compute_cluster_consensus(...) -> BundleDecision`
Keep `smooth_burst_consensus` as a clean coordinator function with cognitive complexity < 15.

- [ ] **Step 4: Verify with Cargo test and Clippy**
Run: `cargo clippy --all-targets --all-features -- -D warnings && cargo test --lib`
Expected: PASS with 0 warnings and 0 errors.

- [ ] **Step 5: Commit**
```bash
git add Cargo.toml src/burst.rs src/catalog/template.rs
git commit -m "feat(sensors): enforce Rust cognitive complexity and refactor burst consensus"
```

---

### Task 3: Rust Architectural Boundary Contract Test

**Files:**
- Create: `tests/test_suite/test_architecture_contracts.rs`
- Modify: `tests/test_suite/main.rs`
- Test: `tests/test_suite/test_architecture_contracts.rs`

**Interfaces:**
- Consumes: `src/*.rs` module files
- Produces: Deterministic integration test asserting inward layering invariants:
  - `src/types.rs` cannot import from `src/scanner.rs`, `src/storage/`, or `src/catalog/`.
  - `src/storage/` cannot import from `src/catalog/`.
  - `src/scanner.rs` cannot import from `src/catalog/`.

- [ ] **Step 1: Write the failing architectural test**
Create `tests/test_suite/test_architecture_contracts.rs` that reads source files in `src/` and checks for illegal `use crate::` dependencies according to the layer hierarchy.

- [ ] **Step 2: Register test in `tests/test_suite/main.rs` and verify failure if rules are broken**
Temporarily test against a forbidden import to verify it catches violations, then assert our clean architecture.

- [ ] **Step 3: Run the test suite**
Run: `cargo test --test test_suite test_architecture_contracts`
Expected: PASS.

- [ ] **Step 4: Commit**
```bash
git add tests/test_suite/test_architecture_contracts.rs tests/test_suite/main.rs
git commit -m "feat(sensors): add Rust architectural module layering contract test"
```

---

### Task 4: Enriched Self-Correction Guidance in Pipeline Scripts

**Files:**
- Modify: `scripts/verify.sh:40-105`
- Modify: `scripts/verify_python.sh:35-85`

**Interfaces:**
- Consumes: Exit codes and outputs from Clippy, Mypy, Ruff, and Cargo test.
- Produces: Actionable prompt-like guidance for coding agents when any sensor fails.

- [ ] **Step 1: Add diagnostic traps and guidance in `scripts/verify_python.sh`**
When `ruff` or `mypy` fails, output structured agent-friendly instructions explaining how to refactor without bypassing rules (e.g. "Do not add # noqa. Extract helpers or add explicit type annotations").

- [ ] **Step 2: Add diagnostic traps and guidance in `scripts/verify.sh`**
When Clippy, `cargo check`, or `cargo test` fails, print clear self-correction instructions explaining the root causes (unwrap prohibitions, Apple Silicon resource bounds, doc comment syntax).

- [ ] **Step 3: Verify the scripts execute cleanly on the healthy codebase**
Run: `./scripts/verify.sh`
Expected: PASS all stages cleanly.

- [ ] **Step 4: Commit**
```bash
git add scripts/verify.sh scripts/verify_python.sh
git commit -m "feat(sensors): add enriched self-correction guidance to verification pipelines"
```

---

### Task 5: Playwright E2E UI Smoke Sensor & Standalone Mutation Testing Script

**Files:**
- Modify: `scripts/verify.sh`
- Create: `scripts/run_mutants.sh`
- Test: Playwright smoke execution & `run_mutants.sh` preflight

**Interfaces:**
- Consumes: `tests/e2e/mock_directory_catalog.spec.ts`
- Produces: Integrated fast E2E smoke test in `verify.sh` stage [7/8] and targeted `cargo-mutants` helper script.

- [ ] **Step 1: Create `scripts/run_mutants.sh`**
Write a script that executes `cargo mutants` targeted at `src/types.rs` and `src/burst.rs`, respecting `CARGO_BUILD_JOBS` and providing installation instructions if `cargo-mutants` is missing.

- [ ] **Step 2: Integrate Playwright E2E smoke test into `scripts/verify.sh`**
Add stage `[7/8]` before doc checks:
If `tests/e2e/node_modules` exists, run:
`npm --prefix tests/e2e test -- mock_directory_catalog.spec.ts --project=chromium --timeout=15000`
If missing, emit a yellow warning and continue.

- [ ] **Step 3: Test both scripts**
Run: `chmod +x scripts/run_mutants.sh && ./scripts/verify.sh`
Expected: PASS all 8 stages.

- [ ] **Step 4: Commit**
```bash
git add scripts/run_mutants.sh scripts/verify.sh
git commit -m "feat(sensors): integrate Playwright smoke test and targeted mutation runner"
```

---

### Task 6: Final Verification & Documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-11-17-maintainability-sensors-design.md` (Update status to APPROVED/IMPLEMENTED)
- Modify: `AGENTS.md` (Document new sensor rules and the mutation test script)

- [ ] **Step 1: Update AGENTS.md with sensor guidance and rules**
Document the new cognitive complexity thresholds, Python complexity linters, and `scripts/run_mutants.sh` in `AGENTS.md`.

- [ ] **Step 2: Run complete 8-stage verification pipeline**
Run: `./scripts/verify.sh`
Expected: Complete green pass across all 8 stages.

- [ ] **Step 3: Commit**
```bash
git add docs/superpowers/specs/2026-11-17-maintainability-sensors-design.md AGENTS.md
git commit -m "docs(sensors): record maintainability sensors in AGENTS.md and mark spec implemented"
```
