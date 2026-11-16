# Network-Mount Resilient SQLite & Unambiguous Catalog Scripts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate SQLite error code 14 (`SQLITE_CANTOPEN`) on NFS/network-mounted manifests and ensure parent catalog correction batch scripts generate unambiguous relative paths.

**Architecture:** 
1. In `src/db.rs`, make `ManifestDb::open_or_create` resilient to network filesystem limitations by setting `busy_timeout` and gracefully falling back to rollback journal mode (`TRUNCATE` or `DELETE`) if WAL mode cannot acquire POSIX shared memory or errors with `SQLITE_CANTOPEN`.
2. In `src/organizer/batch.rs`, finalize the parent rollup SQLite database in `DELETE` journal mode before committing it to disk so network mounts can open it without POSIX shm locks.
3. In `src/catalog/template.rs`, ensure generated `avicull correct` commands prefer `relative_path` when present in multi-directory catalogs to prevent ambiguous matches across trips.

**Tech Stack:** Rust (rusqlite, anyhow), HTML/JS (template.rs), Cargo test harness

**Spec:** Issue where `avicull correct` on `/Volumes/pictures` fails with `Error code 14: Unable to open the database file` due to WAL mode on NFS mounts, plus ambiguous bare stems across multi-trip parent catalogs.

## Global Constraints
- Strict Source Immutability in Default Preferred Mode (`--mode in-place`)
- Zero compiler warnings, zero clippy warnings (`clippy::all`, `clippy::pedantic`)
- `unsafe_code = "deny"`
- TDD required: write failing tests first, then implement.

---

### Task 1: Resilient SQLite Journal Mode Fallback in `ManifestDb`

**Files:**
- Modify: `src/db.rs`
- Test: `tests/test_suite/test_db_network_resilience.rs`
- Register: `tests/test_suite/main.rs`

- [ ] Write failing test in `tests/test_suite/test_db_network_resilience.rs` verifying that `ManifestDb::open_or_create` succeeds even when WAL mode is prohibited or when opening databases with DELETE/TRUNCATE mode.
- [ ] Run test suite and observe failure/missing modules.
- [ ] Update `src/db.rs` to set `busy_timeout = 5000` and fall back gracefully from `PRAGMA journal_mode = WAL` to `TRUNCATE` or `DELETE` if WAL mode fails or returns error 14/busy.
- [ ] Re-run tests to confirm green.

---

### Task 2: Finalize Parent Rollup Manifest in DELETE Mode in `organizer/batch.rs`

**Files:**
- Modify: `src/organizer/batch.rs`
- Test: `tests/test_suite/test_organize_all.rs`

- [ ] Write failing test or assertion in `test_organize_all.rs` asserting the rolled-up parent manifest is saved in rollback journal mode (`DELETE` or `TRUNCATE`), not WAL mode, with no lingering `-shm` or `-wal` dependencies.
- [ ] Observe test failure.
- [ ] In `src/organizer/batch.rs`, execute `PRAGMA journal_mode = DELETE;` and checkpoint before closing and copying the staged parent manifest.
- [ ] Re-run tests to confirm green.

---

### Task 3: Unambiguous File Paths in HTML Catalog Correction Commands

**Files:**
- Modify: `src/catalog/template.rs`
- Test: `tests/test_suite/test_catalog.rs`

- [ ] Write failing test in `tests/test_suite/test_catalog.rs` verifying that when `relative_path` is present on an item, `buildMoveCommandsForItem` generates `--file "<relative_path>"` rather than just `--file "<original_stem>"`.
- [ ] Run test and observe failure.
- [ ] In `src/catalog/template.rs`, update `buildMoveCommandsForItem` to prioritize `item.relative_path` over bare stem when generating `avicull correct`.
- [ ] Verify test passes.

---

### Task 4: Full Codebase Verification and Pre-Push Checks

- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`.
- [ ] Run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- [ ] Run `./scripts/verify.sh`.
