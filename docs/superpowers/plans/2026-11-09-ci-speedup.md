# CI Rust Test Suite Build & Execution Time Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce CI Rust test suite build and execution time from 11+ minutes down to under 2 minutes on warm runs and under 4 minutes on cold runs across all pull requests and main branch builds.

**Architecture:** Eliminate redundant binary compilation in the `test-rust` workflow job (Cargo automatically builds the binary for integration tests that need it), enable `push: branches: [main]` trigger in `pr_checks.yml` to establish and maintain a base GitHub Actions Rust cache for all PRs, tune `profile.dev` dependency optimization in `Cargo.toml` to cut LLVM cold compilation overhead, and integrate `nextest` into `cargo-llvm-cov` for parallel multi-process test execution.

**Tech Stack:** GitHub Actions, Rust (`cargo`, `cargo-llvm-cov`, `cargo-nextest`, `Swatinem/rust-cache`, `taiki-e/install-action`).

**Spec:** Root cause analysis and action plan established in the systematic debugging investigation of GitHub Actions run 42652565832 (Job 129282511655).

## Global Constraints
- `unsafe_code = "deny"`: No unsafe Rust permitted anywhere.
- Source immutability and APFS invariants in AGENTS.md must be strictly preserved.
- The 80% line coverage gate (`--fail-under-lines 80`) must remain enforced and passing.
- All pre-commit/verification pipeline stages (`./scripts/verify.sh`) must pass cleanly (format, clippy, tests, python checks, cargo deny, cargo doc with zero warnings).
- Zero agent meta-comments or procedural artifacts in any committed file.

## Review Focus
1. Integration tests depending on `env!("CARGO_BIN_EXE_avicull")` must continue finding the compiled binary without manual pre-compilation.
2. Workflow YAML syntax for triggers (`push` and `pull_request`) and `taiki-e/install-action` must be valid GitHub Actions schema.
3. `cargo-llvm-cov` coverage calculation and reporting must work identically with `nextest`.
4. Dev profile package overrides in `Cargo.toml` must remain valid Cargo syntax and maintain adequate execution speed for image processing tests.
5. Cache keys and paths must be properly saved and restored without exceeding GitHub Actions cache limits.

---

### Task 1: Dev Profile Optimization in `Cargo.toml`
**Files:**
- Modify: `Cargo.toml:79-81`
- Test: `cargo check` and `cargo test --lib --test test_suite`

- [x] **Step 1: Check existing profile dev settings in `Cargo.toml`**
- [x] **Step 2: Update `Cargo.toml` to set `[profile.dev.package."*"] opt-level = 1` and add targeted `[profile.dev.package.image] opt-level = 2` and `[profile.dev.package.blake3] opt-level = 2`**
- [x] **Step 3: Run `cargo check --all-targets` and verify compilation**
- [x] **Step 4: Commit changes (`perf(cargo): tune dev profile package optimization levels for faster builds`)**

---

### Task 2: Remove Redundant Binary Build Step in `pr_checks.yml`
**Files:**
- Modify: `.github/workflows/pr_checks.yml:60-63`
- Test: Verify with `cargo test --test test_mock_directory_structure_e2e --no-run` that Cargo builds the binary dependency automatically

- [x] **Step 1: Inspect lines 58-68 of `.github/workflows/pr_checks.yml`**
- [x] **Step 2: Remove the `Build local binary for E2E tests` step (`cargo build --bin avicull`)**
- [x] **Step 3: Verify the step order flows directly from `Rust Cache` to `Install cargo-llvm-cov`**
- [x] **Step 4: Commit changes (`ci(workflows): remove redundant cargo build before llvm-cov in test-rust`)**

---

### Task 3: Enable Base Branch Caching on `main` in `pr_checks.yml`
**Files:**
- Modify: `.github/workflows/pr_checks.yml:3-8`
- Test: Verify YAML structure

- [x] **Step 1: Add `push: branches: [main]` under the `on:` block in `.github/workflows/pr_checks.yml`**
- [x] **Step 2: Ensure concurrency group correctly handles both PR and main branch pushes without canceling main branch cache saves**
- [x] **Step 3: Commit changes (`ci(workflows): trigger PR checks on push to main to maintain warm base cache`)**

---

### Task 4: Integrate `cargo-nextest` with `cargo-llvm-cov` in `pr_checks.yml`
**Files:**
- Modify: `.github/workflows/pr_checks.yml:60-70`
- Test: Verify `taiki-e/install-action` and `cargo llvm-cov nextest` command options

- [x] **Step 1: Update `Install cargo-llvm-cov` to install `cargo-llvm-cov,nextest` via `taiki-e/install-action@v2`**
- [x] **Step 2: Update the coverage run step to use `cargo llvm-cov nextest --fail-under-lines 80 --lib --test test_suite --test test_mock_directory_structure_e2e`**
- [x] **Step 3: Commit changes (`ci(workflows): use nextest runner with cargo-llvm-cov for parallel test execution`)**

---

### Task 5: Local Verification Pipeline
**Files:**
- Test: Run `./scripts/verify.sh`
- Verify: Rust formatting, Clippy, unit & integration tests, python tests, cargo deny, and cargo doc with `-D warnings`

- [x] **Step 1: Run `./scripts/verify.sh`**
- [x] **Step 2: Ensure 0 warnings and all test suites pass**
- [x] **Step 3: Verify git status is clean**
