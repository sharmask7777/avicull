# Build Speed Optimization & Test Harness Consolidation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Accelerate the Rust test compilation and linking times by >4x, reduce disk I/O from 6.4 GB down to <1 GB, and eliminate MacBook CPU pegging and thermal throttling while preserving 100% test coverage and blocking gates.

**Architecture:**
1. Group the 40 fragmented integration test files into unified test harnesses (`tests/integration/mod.rs` or focused subsystem binaries) so `cargo test` links 2-3 Mach-O binaries instead of 40 separate binaries.
2. Deduplicate test passes in `.githooks/pre-push` by having `cargo llvm-cov` serve as the unified test and coverage runner.
3. Configure `CARGO_BUILD_JOBS` and cargo runner thread caps to keep efficiency cores free for macOS system responsiveness.

---

## Global Constraints
- Strict Source Immutability (never modify files in `--input`).
- APFS zero-byte hardlink integrity (`dst_meta.ino() == src_ino`).
- Blocking code coverage gate remains at $\ge 80\%$ (aspiring to $>95\%$).
- All test suites must remain green.
- Zero agent meta-comments in code.

---

## Tasks

- [ ] **Task 1: Benchmark baseline compile and link time**
  - Measure baseline `cargo test --no-run` time, binary count, and `target/debug/deps` disk size.

- [ ] **Task 2: Consolidate integration tests into unified test harness**
  - Create `tests/integration/` directory containing the consolidated test modules.
  - Create runner entrypoint `tests/test_main.rs` (or keeping standalone only those that require independent process flags like `test_e2e_real_samples` and `test_mock_directory_structure_e2e`).
  - Verify all tests compile, link, and pass.

- [ ] **Task 3: Deduplicate `.githooks/pre-push` execution passes**
  - Unify uninstrumented test run and `cargo-llvm-cov` into a single pass that executes tests and validates the coverage gate simultaneously.
  - Avoid redundant rebuilds of debug/release binaries.

- [ ] **Task 4: Cap Cargo worker threads for Apple Silicon thermal and system responsiveness**
  - Configure `.cargo/config.toml` or pre-push environment with `jobs = 6` (or $N_{cores} - 4$) to reserve efficiency cores for macOS desktop operations.

- [ ] **Task 5: Final verification & multi-agent review crew**
  - Benchmark new compilation & linking times.
  - Run `./.githooks/pre-push` to confirm all 8 gates pass.
  - Dispatch verification crew.
