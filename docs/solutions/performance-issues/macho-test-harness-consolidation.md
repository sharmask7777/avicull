---
title: "Consolidating Integration Test Harnesses to Prevent macOS Mach-O Linker Contention"
date: "2026-11-08"
category: "performance-issues"
module: "tests"
problem_type: "performance_issue"
component: "testing_framework"
severity: "high"
symptoms:
  - "Incremental test runs taking 45-90 seconds due to repetitive ld64 linking passes"
  - "Target debug directory bloating by over 5.8 GB from redundant debug symbols"
  - "Thermal throttling and CPU core starvation on macOS Apple Silicon during pre-push checks"
root_cause: "test_isolation"
resolution_type: "workflow_improvement"
tags:
  - "macho-linker"
  - "apple-silicon"
  - "cargo-test"
  - "test-consolidation"
---

# Consolidating Integration Test Harnesses to Prevent macOS Mach-O Linker Contention

## Problem
In early iterations of `avicull`, every new integration test was placed into an independent root file under `tests/` (such as historical targets like `test_burst.rs` or `test_catalog.rs` before being relocated to `tests/test_suite/`). Cargo treats each root file in `tests/*.rs` as an autonomous integration test target, forcing Apple clang and `ld64` to compile and link separate Mach-O binaries for every file. As the test suite expanded beyond 38 test targets, test iteration times exceeded 45 seconds, the `target/debug/deps` directory bloated by more than 5.8 GB, and Apple Silicon efficiency cores were swamped.

## Symptoms
- `cargo test` execution times stalled for prolonged periods during linking phases.
- Compiling clean or incremental test runs produced dozens of massive Mach-O binaries in `target/debug/deps/`.
- Developer machines experienced fan spin-up and thermal throttling on Apple Silicon during pre-push hook runs.

## What Didn't Work
- Running `cargo test --jobs 1` serialized the linker invocations to prevent thermal overload, but increased total build wait time to over two minutes.
- Enabling `lld` or mold linkers was not portable on macOS Darwin due to Mach-O and code signing requirements.
- Splitting tests into arbitrary workspace sub-crates added unnecessary crate graph complexity and dependency compilation overhead.

## Solution
Consolidate all integration test modules under a single integration test runner located at `tests/test_suite/main.rs`. Under Rust 2021 edition directory target semantics, files in `tests/test_suite/*.rs` resolve cleanly as submodules:

```rust
// tests/test_suite/main.rs
//! # Consolidated Integration Test Runner
//! Aggregates all integration tests into a single compiled binary.

mod test_burst;
mod test_catalog;
mod test_classifier;
mod test_detector;
mod test_geo;
mod test_spec_conformance;
// ... all submodules linked into one Mach-O binary
```

Only tests that require external process environments or isolated CLI execution—such as `tests/test_mock_directory_structure_e2e.rs`—remain as standalone root targets.

In addition, hardware-aware build concurrency was introduced to dynamically cap test jobs to physical performance cores (`hw.perflevel0.logicalcpu`), leaving efficiency cores available for the macOS desktop environment.

## Why This Works
By linking the entire test suite into a single `test_suite` Mach-O binary:
1. `ld64` executes exactly once for all integrated tests instead of 38+ times.
2. Duplicate static symbols from dependencies (e.g., `image`, `rusqlite`, `serde`) are linked once into a unified binary, saving >5.8 GB of disk space.
3. Incremental test cycle times drop from >44s to under 5s on Apple Silicon.

## Prevention
- `AGENTS.md` and `CODING_STANDARDS.md` mandate that all new integration test files must be placed under `tests/test_suite/` and registered as submodules in `tests/test_suite/main.rs`.
- Root test files in `tests/` are restricted to isolated external process suites.

## Related Issues
- Solved in PR #35
- Documented in `AGENTS.md` and `tests/test_suite/main.rs`
