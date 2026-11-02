# Implementation Plan: Agentic AI-Native Build-Time Guardrails

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement strict build-time compiler, linter, and supply-chain guardrails across the `avicull` codebase to make it agentic AI-native (Issue #36).

**Architecture:** Configure crate-level strict lints in `Cargo.toml` (`clippy::all = "deny"`, `clippy::pedantic = "deny"`, `clippy::unwrap_used = "deny"`, `clippy::panic = "deny"`, `clippy::todo = "deny"`, `clippy::redundant_clone = "deny"`, `forbid(unsafe_code)`) with sensible lint overrides. Replace all production `.unwrap()` calls with structured error handling. Add `deny.toml` for supply-chain vulnerability and license auditing. Add `scripts/verify.sh` and update `.githooks/pre-push` and `.github/workflows/pr_checks.yml`.

**Tech Stack:** Rust (1.99.1), Cargo, Clippy, cargo-deny, bash.

**Spec:** [Technical Design: Agentic AI-Native Build-Time Guardrails](file:///Users/shaleensharma/.gemini/antigravity/brain/f679e5c6-f797-469a-b297-c4a2444bcd08/agentic_guardrails_design.md)

## Global Constraints

- **Strict Source Immutability**: Production and scanner code must preserve source immutability.
- **Zero-Byte Output Hardlinks**: APFS hard links must preserve inode identity (`dst_meta.ino() == src_ino`).
- **No Agent Meta-Artifacts**: Zero procedural comments (`// Step 1:`) in committed code.
- **Test Target Relaxation**: Test targets (`tests/test_suite/**`, `tests/test_mock_directory_structure_e2e.rs`, `tests/test_e2e_real_samples.rs`) allow unwraps and panics.
- **Documentation Standards**: Must pass `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` with 0 warnings.
- **Blocking Coverage Gate**: Must pass `cargo llvm-cov --fail-under-lines 80`.

## Review Focus

- Mutex lock poison handling in `src/db.rs` must not panic; map poison to `AvicullError::DatabaseError`.
- Path string conversions (`to_str()`) in `src/update.rs` must handle invalid UTF-8 without panicking.
- Path parent and component lookups in `src/burst.rs`, `src/storage/mod.rs`, and `src/organizer/reconcile.rs` must handle edge roots gracefully.
- `deny.toml` licenses must allow all dependencies of `image`, `rusqlite`, `clap`, and `chrono`.
- Pre-push and CI pipelines must remain aligned on all verification gates.

---

### Task 1: Eliminate Redundant Clones and Sanitize Option/Result Unwraps in Storage & Utility Modules

**Files:**
- Modify: `src/main.rs:1660-1695`
- Modify: `src/storage/mod.rs:115-130`
- Modify: `src/burst.rs:125-140`
- Modify: `src/organizer/reconcile.rs:110-125`
- Modify: `src/update.rs:285-350`
- Test: `tests/test_suite/test_update.rs`
- Test: `tests/test_suite/test_burst.rs`
- Test: `tests/test_suite/test_reconcile_moves.rs`

**Interfaces:**
- Consumes: `AvicullError`, `Result<T>`
- Produces: Poison-free and panic-free helper methods in non-db utility modules.

- [ ] **Step 1: Write a unit test ensuring path conversion failure in `update.rs` returns `Err` without panicking**
- [ ] **Step 2: Run test to verify it executes and passes or fails as expected**
- [ ] **Step 3: Replace `.unwrap()` and redundant clones in `src/main.rs`, `src/storage/mod.rs`, `src/burst.rs`, `src/organizer/reconcile.rs`, and `src/update.rs`**
- [ ] **Step 4: Run the test suite on these modules to verify zero regressions**
- [ ] **Step 5: Commit changes**

---

### Task 2: Refactor Mutex Lock Handling in `src/db.rs` to Eliminate Panic/Unwrap on Poison

**Files:**
- Modify: `src/db.rs`
- Test: `tests/test_suite/test_db.rs`

**Interfaces:**
- Consumes: `rusqlite::Connection`, `std::sync::Mutex`, `AvicullError`
- Produces: `fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, AvicullError>`

- [ ] **Step 1: Write a unit test in `src/db.rs` testing poisoned lock recovery or graceful database error mapping**
- [ ] **Step 2: Run test to verify behavior**
- [ ] **Step 3: Introduce `lock_conn(&self)` helper in `src/db.rs` and refactor all 35+ occurrences of `self.conn.lock().unwrap()` to `self.lock_conn()?`**
- [ ] **Step 4: Run `cargo test --test test_suite -- test_db` to verify all 12 database tests pass**
- [ ] **Step 5: Commit changes**

---

### Task 3: Configure Strict Compiler and Clippy Lints in `Cargo.toml` and Module Headers

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `tests/test_suite/main.rs`
- Modify: `tests/test_mock_directory_structure_e2e.rs`
- Modify: `tests/test_e2e_real_samples.rs`

**Interfaces:**
- Consumes: Cargo lint configuration system
- Produces: Hard build-time compiler gate with `clippy::all = "deny"`, `clippy::pedantic = "deny"`, `clippy::unwrap_used = "deny"`, `clippy::panic = "deny"`, `forbid(unsafe_code)`.

- [ ] **Step 1: Add `[lints.rust]` and `[lints.clippy]` configuration to `Cargo.toml` with pedantic overrides (e.g. `doc_markdown`, `similar_names`, `too_many_lines`, `cast_possible_truncation`, `module_name_repetitions`, `must_use_candidate`, `missing_errors_doc`, `missing_panics_doc`)**
- [ ] **Step 2: Add `#![allow(clippy::unwrap_used, clippy::panic)]` to `tests/test_suite/main.rs` and E2E test files**
- [ ] **Step 3: Run `cargo clippy --all-targets -- -D warnings` and fix any remaining pedantic lint items**
- [ ] **Step 4: Verify `cargo clippy` runs clean with 0 warnings across all targets**
- [ ] **Step 5: Commit changes**

---

### Task 4: Configure Dependency Security & License Auditing (`deny.toml`)

**Files:**
- Create: `deny.toml`
- Test: Local execution of `cargo deny check`

**Interfaces:**
- Consumes: `cargo-deny` CLI
- Produces: Validated `deny.toml` policy rejecting vulnerabilities and enforcing compatible open source licenses.

- [ ] **Step 1: Create `deny.toml` with advisory checks, license checks (allowing MIT, Apache-2.0, 0BSD, Unicode-3.0, ISC, Zlib, CC0-1.0, OpenSSL), and bans**
- [ ] **Step 2: Run `cargo deny check` to verify policy evaluation**
- [ ] **Step 3: Refine `deny.toml` allows/skips until `cargo deny check` passes with 0 errors**
- [ ] **Step 4: Commit `deny.toml`**

---

### Task 5: Create Unified Verification Pipeline Script (`scripts/verify.sh`) and Update Pre-Push Hook & CI

**Files:**
- Create: `scripts/verify.sh`
- Modify: `.githooks/pre-push`
- Modify: `.github/workflows/pr_checks.yml`
- Modify: `AGENTS.md`
- Modify: `CONTRIBUTING.md`

**Interfaces:**
- Consumes: All verification tools (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo deny`, `cargo doc`, `cargo-llvm-cov`)
- Produces: Automated single-command and CI build verification.

- [ ] **Step 1: Create `scripts/verify.sh` implementing all 7 verification commands from Issue #36 and make it executable (`chmod +x scripts/verify.sh`)**
- [ ] **Step 2: Add `cargo-deny` checking to `.githooks/pre-push` and install `cargo-deny` in `.github/workflows/pr_checks.yml`**
- [ ] **Step 3: Update `AGENTS.md` and `CONTRIBUTING.md` documenting strict clippy lints, `deny.toml`, and `scripts/verify.sh`**
- [ ] **Step 4: Run `./scripts/verify.sh` and `./.githooks/pre-push` to verify all gates succeed**
- [ ] **Step 5: Commit changes**

---

### Task 6: Final Review Crew Audit, Push, and PR Creation

**Files:**
- Inspect: All touched files

- [ ] **Step 1: Dispatch multi-agent review crew (`correctness-safety-reviewer`, `rust-readability-reviewer`, `spec-conformance-reviewer`)**
- [ ] **Step 2: Resolve any review findings**
- [ ] **Step 3: Push branch `feat/build-time-agentic-guardrails` and open pull request with `gh pr create`**
- [ ] **Step 4: Monitor GitHub Actions CI checks to green completion**
