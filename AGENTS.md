# `AGENTS.md` — Agent Instructions for `avicull`

Operational guide for AI coding agents working in `avicull` (local-first Rust + Python photo organizer for macOS Apple Silicon). Keep changes minimal, safe, and strictly verified.

## 1. Canonical Reference Map

Read the relevant source-of-truth document before designing, editing, or reviewing:

| Document | Scope |
|---|---|
| [`CONCEPTS.md`](CONCEPTS.md) | Shared domain vocabulary (`AssetBundle`, `CatalogFilesDirectory`, `DecoupledViewProjection`, etc.) — consult before naming or refactoring types |
| [`CODING_STANDARDS.md`](CODING_STANDARDS.md) | Mandatory Rust, Python, ML inference, build, and UI defect-prevention rules |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Dev setup, 8-tier testing strategy, CI checks, taxonomy/gazetteer updates, and release process |
| [`REVIEW.md`](REVIEW.md) | Code review severity definitions, evidence requirements, and explicit exclusions |
| [`docs/README.md`](docs/README.md) | Index of designs, specs, plans, SDD ledgers, UI brainstorms, and [`docs/solutions/`](docs/solutions/) (check before debugging known issues) |
| [`skills/avian-model-updater/SKILL.md`](skills/avian-model-updater/SKILL.md) | Workflow for upgrading neural backbones, detectors, taxonomies, and spatial priors |

## 2. Non-Negotiable Tripwires

- **Default `--mode in-place` is read-only**: Never move, rename, modify, or unlink source photos/sidecars in `--input`; write only to `<input>/avicull_catalog_files/` (`.avicull/` compat) and `<input>/catalog.html`.
- **Opt-in `--mode in-place-rename` & `--mode hierarchy`**: Rename primary+sidecar pairs atomically with SQLite journaling (`avicull clean` rollback); in `hierarchy`, use zero-byte `std::fs::hard_link` (`dst_meta.ino() == src_ino`) supporting parent/child coexistence.
- **Traversal & Dataless Guards**: Always skip `birds_hierarchy`, `avicull_catalog_files`, `.avicull`, `*.photoslibrary`, `@eaDir`, and `.*`; never eagerly open Darwin `SF_DATALESS` (`0x40000000`) files during pre-scans.
- **Decoupled View & Performance Modes**: Keep `render-catalog` / `organize-all --view-only` pure SQLite->HTML with zero image I/O, honoring `DualThumbnailStrategy` (`CONCEPTS.md`), depth-8 directory-only NAS probes, incremental bundle caching, and `--reclassify-mode fast` as default.
- **Clean & Generic Code**: No hardcoded personal paths, trip names, or regional heuristics; no procedural AI meta-comments (`// Step 1: ...`).

## 3. Workflow & Verification Commands

- **TDD Iron Law**: Never write production code without a failing test first. Register all Rust integration tests in `tests/test_suite/main.rs` (never standalone `tests/test_*.rs` root files unless isolated process env is required).
- **Inner-Loop Tests**:
  ```bash
  cargo test --test test_suite -- <test_name>
  uv run pytest <test_file>
  ```
- **Language & Asset Rules**: Follow `CODING_STANDARDS.md` and `CONTRIBUTING.md` (`Cargo.toml` denies `unsafe_code`, `pedantic`, `unwrap_used`, `panic`, `todo`, `redundant_clone`, `cognitive_complexity`; Python via `uv run` with `ruff`, `mypy scripts/`, `lint-imports`, `pytest --cov-fail-under=80`; taxonomy/gazetteer via `uv run python scripts/build_gazetteer.py` & `uv run pytest tests/test_build_gazetteer.py`).
- **Pre-Commit / Pre-Push Gates**:
  ```bash
  ./scripts/verify.sh          # 8-stage Rust, Python, cargo-deny, Playwright, and rustdoc gate
  ./.githooks/pre-push         # Full gate including >=80% llvm-cov and real/mock binary E2E
  ```

## 4. Pre-Completion Review & Compounding

- **Concurrent Review Crew**: Before concluding any feature, refactor, or optimization, dispatch `correctness-safety-reviewer`, `rust-readability-reviewer`, and `spec-conformance-reviewer` as concurrent review subagents (governed by [`REVIEW.md`](REVIEW.md)) and remediate all findings.
- **Compounding**: Offer once at completion to invoke `ce-compound` only when non-obvious, durable project reasoning emerged that is absent from code, tests, or docs.
