# Implementation Plan: Python Agentic AI-Native Guardrails & Blocking Coverage

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement strict Python build-time compiler/linter guardrails, immutable dataclasses, pure functions, and a blocking $\ge 80\%$ test coverage gate across all Python scripts and tests.

**Architecture:** Configure `pyproject.toml` with `ruff`, `mypy`, and `pytest-cov`. Refactor `scripts/*.py` to replace ad-hoc mutable dicts with pure functions and `@dataclass(frozen=True)` immutable structures. Augment unit tests to achieve $\ge 80\%$ line coverage across `scripts/`. Integrate `scripts/verify_python.sh` into `scripts/verify.sh`, `.githooks/pre-push`, and `.github/workflows/pr_checks.yml`.

**Tech Stack:** Python 3.10+, `uv`, `ruff`, `mypy`, `pytest`, `pytest-cov`, PyTorch, OpenCLIP, Safetensors.

**Spec:** [Technical Design: Python Agentic AI-Native Guardrails & Blocking Coverage](file:///Users/shaleensharma/.gemini/antigravity/brain/f679e5c6-f797-469a-b297-c4a2444bcd08/python_guardrails_design.md)

## Global Constraints

- **Source Immutability**: No photo archive modifications.
- **Pure Functions & Immutability**: All data models in `scripts/` must be `@dataclass(frozen=True)` or `NamedTuple`. Functions manipulating coordinates, slugs, taxonomies, and posteriors must be pure.
- **Blocking Coverage Gate**: `--cov-fail-under=80` must pass.
- **Zero Linter/Type Errors**: `ruff check`, `ruff format --check`, and `mypy` must pass with 0 errors.
- **No Agent Meta-Artifacts**: Zero procedural AI comments in code.

---

### Task 1: Configure Python Tooling in `pyproject.toml` and Format/Lint Baseline

**Files:**
- Create: `pyproject.toml`
- Modify: `scripts/*.py`, `tests/test_*.py`, `tests_python/*.py`

**Interfaces:**
- Consumes: `ruff`, `mypy`, `pytest`, `pytest-cov`
- Produces: Formatted and lint-clean codebase conforming to `ruff` rules.

- [x] **Step 1: Create `pyproject.toml` configuring ruff, mypy, and pytest-cov**
- [x] **Step 2: Run `uv run --with ruff ruff check --fix .` and `uv run --with ruff ruff format .`**
- [x] **Step 3: Verify with `uv run --with ruff ruff check .` and `uv run --with ruff ruff format --check .` (0 errors)**
- [x] **Step 4: Commit baseline formatting and tooling config**

---

### Task 2: Refactor `scripts/build_gazetteer.py` with Pure Functions and `@dataclass(frozen=True)`

**Files:**
- Modify: `scripts/build_gazetteer.py`
- Modify: `tests/test_build_gazetteer.py`

**Interfaces:**
- Consumes: Raw hotspot registries and checklist maps
- Produces: `@dataclass(frozen=True) class GazetteerHotspot`, pure functions `validate_coordinates`, `normalize_hotspot`, `deduplicate_hotspots`, `build_gazetteer_data`

- [x] **Step 1: Write failing unit tests in `tests/test_build_gazetteer.py` for pure validation, normalization, and deduplication functions**
- [x] **Step 2: Verify test failure**
- [x] **Step 3: Implement `GazetteerHotspot` and pure functions in `scripts/build_gazetteer.py`**
- [x] **Step 4: Run `pytest tests/test_build_gazetteer.py --cov=scripts/build_gazetteer --cov-report=term-missing` and verify 90%+ coverage**
- [x] **Step 5: Commit changes**

---

### Task 3: Refactor `scripts/build_taxonomy_embeddings.py` and Fix `tests/test_infer_regional.py`

**Files:**
- Modify: `scripts/build_taxonomy_embeddings.py`
- Modify: `tests/test_infer_regional.py`
- Modify: `tests_python/test_build_taxonomy.py`

**Interfaces:**
- Consumes: Taxonomy metadata and embedding extraction
- Produces: `@dataclass(frozen=True) class TaxonomyEntry`, pure slug/hash functions, mockable engine fixture

- [x] **Step 1: Update `tests/test_infer_regional.py` to create a synthetic mock taxonomy/checklist fixture so it passes offline**
- [x] **Step 2: Refactor `scripts/build_taxonomy_embeddings.py` with frozen dataclasses and pure batch/slug functions**
- [x] **Step 3: Add unit tests in `tests_python/test_build_taxonomy.py` testing pure functions and metadata compilation**
- [x] **Step 4: Run pytest across these tests and verify high coverage**
- [x] **Step 5: Commit changes**

---

### Task 4: Refactor `scripts/infer.py` & `scripts/export_models.py` with Immutable Records and Pure Math Helpers

**Files:**
- Modify: `scripts/infer.py`
- Modify: `scripts/export_models.py`
- Modify: `tests_python/test_infer.py`
- Create: `tests_python/test_export_models.py`

**Interfaces:**
- Consumes: Bounding box predictions and confidence distributions
- Produces: `@dataclass(frozen=True) class DetectionBox`, `InferencePrediction`, pure IoU and posterior scaling functions

- [x] **Step 1: Write unit tests for pure detection box math, clipping, and posterior multiplier scaling in `tests_python/test_infer.py`**
- [x] **Step 2: Implement frozen dataclasses and pure functions in `scripts/infer.py`**
- [x] **Step 3: Add unit tests for `scripts/export_models.py` CLI parser and export helper validation in `tests_python/test_export_models.py`**
- [x] **Step 4: Verify test suite passes and measure overall coverage across `scripts/`**
- [x] **Step 5: Commit changes**

---

### Task 5: Implement `scripts/verify_python.sh`, Integrate Pre-Push Hook, CI, and Documentation

**Files:**
- Create: `scripts/verify_python.sh`
- Modify: `scripts/verify.sh`
- Modify: `.githooks/pre-push`
- Modify: `.github/workflows/pr_checks.yml`
- Modify: `AGENTS.md`
- Modify: `CONTRIBUTING.md`

**Interfaces:**
- Consumes: `uv`, `ruff`, `mypy`, `pytest`, `pytest-cov`
- Produces: Automated build-time Python verification and coverage enforcement.

- [x] **Step 1: Create executable `scripts/verify_python.sh` running ruff, mypy, and pytest with `--cov-fail-under=80`**
- [x] **Step 2: Integrate `scripts/verify_python.sh` into `scripts/verify.sh` and Step `[4/8]` of `.githooks/pre-push`**
- [x] **Step 3: Update `.github/workflows/pr_checks.yml` (`python-and-gazetteer` job) with ruff, mypy, and coverage gates**
- [x] **Step 4: Document Python guardrails, pure functions, and frozen dataclasses in `AGENTS.md` and `CONTRIBUTING.md`**
- [x] **Step 5: Run `./scripts/verify.sh` to ensure all 8 verification stages pass**
- [x] **Step 6: Commit changes**

---

### Task 6: Final Review Crew Audit, Push, and PR Creation

**Files:**
- Inspect: All touched Python and CI files

- [ ] **Step 1: Dispatch multi-agent review crew (`correctness-safety-reviewer`, `rust-readability-reviewer`, `spec-conformance-reviewer`)**
- [ ] **Step 2: Resolve any review findings**
- [ ] **Step 3: Push branch `feat/python-agentic-guardrails-and-coverage` and open PR**
- [ ] **Step 4: Monitor GitHub Actions CI checks to green completion**
