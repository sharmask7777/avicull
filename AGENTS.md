# AGENTS.md — Developer Guidelines & Preferences for `avicull`

Welcome to `avicull`. This document defines the engineering standards, architectural invariants, and non-negotiable user preferences that **all future AI agent iterations and developers must follow**.

---

## 1. Non-Negotiable Core Invariants

1. **Strict Source Immutability**:
   The input archive (`--input`) is strictly read-only. Never move, rename, modify, or unlink any file inside `--input`.
2. **Zero-Byte Output Tree via Hard Links (`std::fs::hard_link`)**:
   Files are organized into an output tree on the same APFS volume using POSIX hard links. Both trees reference identical underlying inodes (`dst_meta.ino() == src_ino`), consuming 0 additional disk bytes for image payloads.
3. **Default Output Directory (`birds_hierarchy`)**:
   If `--output` is omitted, `avicull` defaults to `<input>/birds_hierarchy`. The tool remains customizable via `--output <PATH>`.
4. **Skipped Paths During Traversal**:
   Recursive scans of `--input` must always skip:
   - Any directory named `birds_hierarchy` (preventing infinite loops or scanning previous output).
   - Any directory named `.avicull` (preventing scanning internal SQLite manifests).
   - Any directory ending with `.photoslibrary` (preventing scanning Apple Photos library packages).
   - Any directory starting with `.` (hidden directories like `.DS_Store`) or `@eaDir`.
5. **Parent & Child Multi-Level Coexistence**:
   A trip subdirectory (e.g. `Pictures/Leh/birds_hierarchy`) and its parent directory (e.g. `Pictures/birds_hierarchy`) can both co-exist simultaneously via hard links. The inode link count (`st_nlink`) increments appropriately without conflict.

---

## 2. Mandatory Engineering Methodologies

### A. Test-Driven Development (TDD) is Mandatory & Non-Negotiable
- **Iron Law**: **NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST.**
- Never write or edit implementation code before writing tests.
- This rule applies equally to Rust core code, CLI options, Python inference scripts, and data pipeline helpers.
- **Workflow**:
  1. **RED**: Write a failing unit or integration test that reproduces the defect, tests the edge case, or defines the new behavior.
  2. **Verify Failure**: Run `cargo test` (or `pytest`) and verify the test fails for the expected reason. Do not proceed until you have observed the failure.
  3. **GREEN**: Write the minimal implementation code necessary to make the test pass.
  4. **Verify Green**: Re-run the test suite and confirm the new test passes and all existing tests stay green.
  5. **REFACTOR**: Clean up, optimize, format (`cargo fmt`), lint (`cargo clippy -- -D warnings`), and document while keeping all tests passing.

### B. Multi-Pronged Testing Strategy
Every pull request or enhancement must be verified across five distinct tiers:
1. **Unit Tests**:
   - Verify pure algorithmic functions in isolation (Bayesian posterior math, temperature scaling, slug sanitization, DMS conversion).
2. **Filesystem Integration Tests (`tests/`)**:
   - Use `tempfile::tempdir` to verify real filesystem operations on disk: APFS inode parity (`st_nlink == 2`), atomic rollbacks, idempotency, sidecar bundling, and collision resolution.
3. **Spec Conformance Tests (`tests/test_spec_conformance.rs`)**:
   - Verify nested & parent coexistence via hard links.
   - Verify incremental auto-detection of newer files.
   - Verify `_No_Bird_Detected` routing tier.
4. **Local Developer Mini-Dataset Inner-Loop Tests (`tests/test_local_user_samples_e2e.rs`)**:
   - Every developer maintains their own local, uncommitted mini-dataset in `tests/fixtures/local_user_samples/` (git-ignored) created from their personal photo library.
   - Run `cargo run -- dataset --source "<PATH_TO_PERSONAL_PHOTOS>"` (or `avicull dataset --source "<PATH>"`) to populate representative trip hierarchies, sidecars, and images (~5-7 MB total).
   - This inner-loop test runs on every compilation and pre-push hook (`[5/7]` and `[6/7]`) to verify full pipeline execution, APFS zero-byte hardlink integrity, HTML catalog generation, and sub-second delta re-runs locally.
5. **Real-World Verification on `/Users/shaleensharma/Pictures/Leh`**:
   - Smoke-test the release binary against real Canon `.CR2` RAW files, `.CR2.xmp` sidecars, and embedded JPEG preview streams.

### C. The Verification Agent Crew Loop
Before concluding any feature, refactor, or optimization, the implementing agent MUST dispatch a multi-agent review crew running concurrently:
- **Agent 1 (`correctness-safety-reviewer`)**: Audits source immutability, APFS hard link invariants, thread safety in Rayon iterations, error handling, and bounds checking.
- **Agent 2 (`rust-readability-reviewer`)**: Audits idiomatic Rust design, function modularity, self-documenting naming, type system ergonomics, and documentation.
- **Agent 3 (`spec-conformance-reviewer`)**: Cross-examines line-by-line compliance against specifications.

Synthesize all crew findings and remediate any flagged issues before reporting completion.

---

## 3. Performance & Build Guidelines

1. **Build Time Optimizations**:
   - Always maintain `split-debuginfo = "unpacked"` in `.cargo/config.toml` and `Cargo.toml` to eliminate `dsymutil` overhead on macOS Apple Silicon.
   - Keep crate feature dependencies minimal (e.g., `image` with `default-features = false` and only `["jpeg", "png", "tiff"]`).
   - Use `[profile.dev.package."*"] opt-level = 2` so dependencies run at release speed while development code compiles in milliseconds.
2. **Incremental Auto-Detection**:
   - When running `organize` on an existing directory, query SQLite for existing bundles.
   - Only execute neural network detection and classification on newly added or modified bundles.
   - Reuse cached predictions and embeddings for previously processed bundles.

---

## 4. Updating Models & Classifiers

When upgrading neural network backbones, bird detectors, taxonomies, or spatial-temporal priors to current cutting-edge architectures, follow the instructions in [`skills/avian-model-updater/SKILL.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/skills/avian-model-updater/SKILL.md).

---

## 5. Clean, Professional Codebase Standards (No Agent Meta-Artifacts)

1. **Meaningful In-Code Comments Only**:
   - Comments in code MUST instruct the human reader on what the code does, its architectural invariants, algorithms, and non-obvious domain logic.
   - **Strictly Prohibited**: Never include procedural agent meta-comments like `// Step 1: ...`, `// Step 2: ...`, `// Step 3: Implement ...`, or descriptions of an AI agent's internal planning phases.
2. **Generic Architecture & Zero Context Leaks**:
   - Code and algorithms must be completely generic. Never hardcode specific trip names, photographer personal directories, or ad-hoc regional heuristics. All spatial and regional prior behavior must be data-driven (e.g. via eBird regional checklists and embedded gazetteers).
3. **End-to-End Mock Testing**:
   - Any new validation behavior (such as location resolution, gazetteer matching, and regional checklist prior filtering) must be covered in end-to-end mock test suites (`tests/test_e2e_*.rs`).

