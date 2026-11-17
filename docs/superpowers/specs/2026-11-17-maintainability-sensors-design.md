# Design Specification: Maintainability Sensors for Coding Agents in `avian_rearranger`

**Date:** 2026-11-17  
**Status:** IMPLEMENTED  
**Reference:** [Maintainability sensors for coding agents (Birgitta Böckeler / Martin Fowler)](https://martinfowler.com/articles/sensors-for-coding-agents.html)

---

## 1. Intent & The "Why"
AI coding agents are highly productive at writing code quickly, but left to their own devices, they exhibit well-known failure modes:
1. **Monolithic sprawl**: Writing 150+ line functions and huge match blocks rather than clean, modular abstractions.
2. **Shallow test illusions**: Generating high statement coverage without robust assertions, creating a false sense of regression safety.
3. **Hidden coupling**: Violating conceptual module layers or importing internals directly.
4. **Thrashing on raw compiler / linter errors**: When a linter fails with a cryptic message, agents often add ad-hoc ignore comments, hack around types, or introduce brittle workarounds instead of proper refactoring.

This design implements a comprehensive sensor harness across Python, Rust, and TypeScript/HTML to catch these failure modes early, enriched with actionable self-correction feedback for coding agents.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Deterministic Computational Sensors for Python**:
  - Enforce McCabe cyclomatic complexity (`C901`), branch limits (`PLR0912`), statement counts (`PLR0915`), and argument limits (`PLR0913`) via `ruff`.
  - Maintain the existing `import-linter` architecture contracts while ensuring new python code adheres to strict modularity.
- **Deterministic Computational Sensors for Rust**:
  - Enable and enforce `clippy::cognitive_complexity` (max threshold 30) across production code.
  - Enforce structural dependency rules between Rust crates/modules (e.g. `storage` and `scanner` must not depend on presentation `catalog`).
- **Enriched Self-Correction Guidance**:
  - Provide domain-specific explanations and concrete remediation advice in `scripts/verify.sh` and `scripts/verify_python.sh` when sensors fail, rather than raw compiler output alone.
- **Regression Effectiveness & Mutation Testing**:
  - Introduce mutation testing tooling (or a targeted mutation test harness) for core critical algorithms (e.g., Bayesian fusion, DMS geographic coordinates, burst clustering) to verify tests actually fail when logic is altered.
- **End-to-End Pipeline Integration**:
  - Incorporate headless Playwright UI smoke verification into the canonical verification flow (`scripts/verify.sh`).

### Non-Goals
- We are not rewriting existing legacy HTML template generators (`src/catalog/template.rs`) into small micro-functions if doing so incurs high runtime overhead or breaks embedded single-file catalog guarantees.
- We are not replacing fast in-session checks with slow 10-minute full-matrix mutation runs; mutation checks must be targeted or runnable on-demand.

### Anti-Goals
- Never allow agents to bypass sensor failures by adding blank `# noqa` or `#[allow(...)]` without documented justification.
- Never slow down the local dev loop (`cargo test`) to an intolerable level on Apple Silicon machines.

---

## 3. Detailed Component Design

### A. Python Complexity & Modularity Sensors
1. **Ruff Ruleset Expansion in `pyproject.toml`**:
   - Add `C901` (cyclomatic complexity, max 15), `PLR0912` (max branches, 12), `PLR0913` (max args, 6), and `PLR0915` (max statements, 50).
   - Refactor `scripts/infer.py` where `process_image` currently triggers these thresholds (e.g., extract bounding box scoring and model forward pass into dedicated private helpers).
2. **Python Self-Correction Wrappers**:
   - In `scripts/verify_python.sh`, trap `ruff` and `mypy` failures and output formatted, actionable hints:
     - For `C901`/`PLR`: "Refactor monolithic functions into pure private helper functions. Do NOT increase thresholds or add blanket # noqa."
     - For `import-linter`: "Architecture contract violated. Check `pyproject.toml` layer boundaries."

### B. Rust Complexity & Architectural Sensors
1. **Clippy Rules in `Cargo.toml`**:
   - Activate `cognitive_complexity = { level = "deny", priority = -1 }` with threshold `30` in `Cargo.toml`.
   - Refactor identified complexity hotspots: `src/burst.rs::smooth_burst_consensus` and `src/catalog/mod.rs::generate_html_catalog_with_policy`.
2. **Module Dependency & Boundary Checks**:
   - Add architectural integrity tests in `tests/test_suite/test_architecture_contracts.rs` that statically inspect module imports or utilize `cargo-modules` to ensure strict inward dependencies (e.g. `types` -> `scanner` -> `classification` -> `storage` -> `catalog`).
3. **Rust Self-Correction Diagnostics in `scripts/verify.sh`**:
   - Enhance failure traps in `scripts/verify.sh` with actionable agent instructions for clippy denies, unwrap violations, and doc warnings.

### C. Mutation Testing for Critical Algorithms
1. **Targeted Mutation Harness**:
   - Integrate `cargo mutants` configuration for core calculation modules (`src/types.rs`, `src/burst.rs`).
   - Add a fast mutation test script `scripts/run_mutants.sh` that targets critical math functions to ensure test assertions catch subtle mutants.

### D. Playwright E2E UI Smoke Sensor
1. **Pipeline Integration**:
   - Add stage `[7/8]` in `scripts/verify.sh` for fast Playwright smoke verification against the fixture catalog (`npm --prefix tests/e2e test`).
   - Provide clear instructions on headless browser requirements.

---

## 4. Verification and Rollout Plan
1. **Baseline**: Run existing `scripts/verify.sh` and record current passing state.
2. **Step 1 (Python)**: Add ruff complexity rules, refactor `scripts/infer.py` hotspots, verify with `scripts/verify_python.sh`.
3. **Step 2 (Rust Clippy & Refactoring)**: Add cognitive complexity linter, refactor `src/burst.rs` and `src/catalog/mod.rs`, ensure zero clippy warnings.
4. **Step 3 (Architectural Contracts)**: Add Rust module boundary contract test.
5. **Step 4 (Enriched Self-Correction)**: Enhance `scripts/verify.sh` and `scripts/verify_python.sh` with guided prompt injections on failure.
6. **Step 5 (E2E & Mutation)**: Connect Playwright test stage and setup `cargo mutants` config.
7. **Final Verification**: Run the unified `./scripts/verify.sh` to ensure all 8 stages pass cleanly.
