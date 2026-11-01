# AGENTS.md — Developer Guidelines & Preferences for `avicull`

Welcome to `avicull`. This document defines the engineering standards, architectural invariants, code documentation guidelines, and non-negotiable user preferences that **all future AI agent iterations and developers must follow**.

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
6. **macOS FileProvider / iCloud Dataless Protection**:
   Files marked with Darwin `SF_DATALESS` (`0x40000000`) must never be opened eagerly during pre-scans to prevent unwanted on-demand cloud hydration.

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
  5. **REFACTOR**: Clean up, optimize, format (`cargo fmt`), lint (`cargo clippy --all-targets -- -D warnings`), document, and verify docs (`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`).

### B. Multi-Pronged Testing Strategy
Every pull request or enhancement must be verified across eight distinct tiers:
1. **Unit Tests**:
   - Verify pure algorithmic functions in isolation (Bayesian posterior math, temperature scaling, slug sanitization, DMS conversion).
2. **Filesystem & Spec Integration Tests (`tests/test_suite/`)**:
   - All unit-like and integration test modules are consolidated under `tests/test_suite/` (declared in `tests/test_suite/main.rs`). This eliminates Mach-O linking contention on macOS Apple Silicon, linking a single test binary instead of dozens.
   - Use `tempfile::tempdir` to verify real filesystem operations on disk: APFS inode parity (`st_nlink >= 2`), atomic rollbacks, idempotency, sidecar bundling, and collision resolution.
   - Spec conformance checks inside `tests/test_suite/test_spec_conformance.rs` verify nested & parent coexistence via hard links, incremental auto-detection of newer files, and `_No_Bird_Detected` routing tier.
3. **Committed Mini-Dataset Mock Directory Structure Tests (`tests/test_mock_directory_structure_e2e.rs`)**:
   - The repository includes a lightweight, fully anonymized mini-dataset in `tests/fixtures/local_user_samples/` (~260 KB total) containing representative trip hierarchies, sidecars, and images. Sensitive EXIF tags (artist, copyright, owner) have been scrubbed.
   - Developers can also refresh or populate custom local datasets via `cargo run -- create-mock-dataset --source "<PATH_TO_PERSONAL_PHOTOS>"`.
   - This mock directory structure test runs on every test suite invocation and pre-push hook (`[5/8]` and `[6/8]`) to verify full pipeline execution, APFS zero-byte hardlink integrity, HTML catalog generation, and sub-second delta re-runs locally.
4. **Playwright End-to-End Visual & Interaction Tests (`tests/e2e/`)**:
   - Every catalog UI, card rendering, and interactive modal flow must be covered by automated Playwright specs.
   - Agents and developers MUST visually and interactively validate catalog changes with bespoke Playwright test scripts (e.g. `tests/e2e/bird_catalog.spec.ts` and `tests/e2e/mock_directory_catalog.spec.ts`).
   - Run `cd tests/e2e && npx playwright test` to exercise all UI flows: multi-select (Cmd/Ctrl+Click, Shift+Click, Cmd+A), search filtering, card display line layout, correction modal autocompletion, and bulk subdirectory location overrides.
   - Commit relevant Playwright specs alongside catalog features to prevent UI and interactive regressions.
5. **Real-World Verification on `/Users/shaleensharma/Pictures/Leh`**:
   - Smoke-test the release binary against real Canon `.CR2` RAW files, `.CR2.xmp` sidecars, and embedded JPEG preview streams (`tests/test_e2e_real_samples.rs`).
6. **Gazetteer & Regional Checklist Integrity Check (`pytest`)**:
   - Validate geographic waypoint schema, regional checklist ISO codes, and coordinate consistency in `tests/test_build_gazetteer.py`.
7. **Build-Time Blocking Code Coverage Metric Gate (`cargo-llvm-cov`)**:
   - Every pull request and push is gated on a minimum blocking line coverage threshold (`cargo llvm-cov --fail-under-lines 80 --lib --test test_suite --test test_mock_directory_structure_e2e`).
   - Aspire to >95% coverage, with 100% target on core domain modules (`types.rs`, `storage/local.rs`, `catalog/template.rs`, `help.rs`).
8. **Rustdoc Documentation Gate**:
   - Must pass `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` with 0 warnings.


### C. The Verification Agent Crew Loop
Before concluding any feature, refactor, or optimization, the implementing agent MUST dispatch a multi-agent review crew running concurrently:
- **Agent 1 (`correctness-safety-reviewer`)**: Audits source immutability, APFS hard link invariants, thread safety in Rayon iterations, error handling, and bounds checking.
- **Agent 2 (`rust-readability-reviewer`)**: Audits idiomatic Rust design, function modularity, self-documenting naming, type system ergonomics, and documentation.
- **Agent 3 (`spec-conformance-reviewer`)**: Cross-examines line-by-line compliance against specifications.

Synthesize all crew findings and remediate any flagged issues before reporting completion.

---

## 3. Rustdoc Standards & Best Practices

1. **Zero-Warning Rustdoc Compilation**:
   - Code must pass `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` with zero warnings.
   - Never leave unclosed HTML tags (e.g., `<script>`, `<OUTPUT>`, `<OriginalStem>`). Always escape them in code spans (`` `<OUTPUT>` ``) or HTML entities.
2. **Module-Level (`//!`) Headers**:
   - Every module file in `src/` must begin with a `//!` documentation header explaining:
     - The module's role in the pipeline.
     - Key architectural invariants guaranteed by the module.
     - Algorithms and domain logic implemented.
     - Concrete examples or usage patterns where appropriate.
3. **Item-Level (`///`) Documentation**:
   - All `pub` structs, enums, variants, traits, methods, and functions must have `///` doc comments.
   - Include sections:
     - `# Arguments`: Parameter explanations.
     - `# Returns`: Return value description.
     - `# Errors`: Conditions under which the function returns `Err`.
     - `# Invariants` / `# Panics`: Any failure guarantees or preconditions.
4. **Tricky Algorithms & Math Rendering**:
   - Complex domain algorithms (e.g., Bayesian posterior fusion, logit temperature scaling, burst consensus pooling) must include mathematical formulations using LaTeX notation (`$...$` and `$$...$$`) and clear prose explanations.

---

## 4. Performance & Build Guidelines

1. **Unified Test Harness Layout**:
   - Always place integration and spec conformance tests under `tests/test_suite/` with their submodules exposed in `tests/test_suite/main.rs`.
   - Never create individual one-off `tests/test_*.rs` root files unless they require isolated external process environments (e.g. `test_mock_directory_structure_e2e.rs` and `test_e2e_real_samples.rs`). This prevents linker (`ld64`) contention, thermal throttling, and multi-gigabyte build directory bloat.
2. **Apple Silicon CPU & Core Preservation**:
   - Pre-push scripts and development tasks should dynamically cap `CARGO_BUILD_JOBS` to physical performance cores (`hw.perflevel0.logicalcpu`, e.g. 6 cores on 6P+4E Apple Silicon), leaving efficiency cores free for macOS desktop and window server responsiveness. Avoid committing fixed machine-specific core counts in `.cargo/config.toml` so CI runners and different machines are not constrained.
3. **Build Time Optimizations**:
   - Always maintain `split-debuginfo = "unpacked"` in `.cargo/config.toml` and `Cargo.toml` to eliminate `dsymutil` overhead on macOS Apple Silicon.
   - Keep crate feature dependencies minimal (e.g., `image` with `default-features = false` and only `["jpeg", "png", "tiff"]`).
   - Use `[profile.dev.package."*"] opt-level = 2` so dependencies run at release speed while development code compiles in milliseconds.
4. **Incremental Auto-Detection**:
   - When running `organize` on an existing directory, query SQLite for existing bundles.
   - Only execute neural network detection and classification on newly added or modified bundles.
   - Reuse cached predictions and embeddings for previously processed bundles.


---

## 5. Updating Models & Classifiers

When upgrading neural network backbones, bird detectors, taxonomies, or spatial-temporal priors to current cutting-edge architectures, follow the instructions in [`skills/avian-model-updater/SKILL.md`](skills/avian-model-updater/SKILL.md).

---

## 6. Clean, Professional Codebase Standards (No Agent Meta-Artifacts)

1. **Meaningful In-Code Comments Only**:
   - Comments in code MUST instruct the human reader on what the code does, its architectural invariants, algorithms, and non-obvious domain logic.
   - **Strictly Prohibited**: Never include procedural agent meta-comments like `// Step 1: ...`, `// Step 2: ...`, `// Step 3: Implement ...`, or descriptions of an AI agent's internal planning phases.
2. **Generic Architecture & Zero Context Leaks**:
   - Code and algorithms must be completely generic. Never hardcode specific trip names, photographer personal directories, or ad-hoc regional heuristics. All spatial and regional prior behavior must be data-driven (e.g. via eBird regional checklists and embedded gazetteers).
3. **End-to-End Mock Testing**:
   - Any new validation behavior (such as location resolution, gazetteer matching, and regional checklist prior filtering) must be covered in end-to-end mock test suites (`tests/test_e2e_*.rs` and `tests/test_mock_directory_structure_e2e.rs`).
