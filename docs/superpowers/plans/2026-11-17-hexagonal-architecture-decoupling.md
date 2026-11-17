# Hexagonal Architecture Decoupling & Architecture Sensor Expansion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple domain orchestration (`src/organizer`) from driving CLI adapters (`src/cli`) and expand architectural contract sensor tests across the codebase with full TDD verification.

**Architecture:** Move configuration parameter structs (`OrganizeArgs`, `OrganizeAllArgs`, `RenderCatalogArgs`) and `ReclassifyMode` from `src/cli.rs` into `src/types.rs` (or domain config). Decouple `src/organizer` completely from `crate::cli`, then enforce comprehensive unidirectional boundaries across all 4 tiers via `tests/test_suite/test_architecture_contracts.rs`.

**Tech Stack:** Rust (2024 edition), Clap (derive), cargo test, clippy, cargo doc.

**Spec:** `docs/superpowers/specs/2026-11-17-hexagonal-architecture-decoupling-spec.md`

## Global Constraints
- `unsafe_code = "deny"`
- Strict TDD: Write failing architecture tests before writing the decoupling implementation.
- All 454+ existing tests must pass deterministically without behavioral regressions.
- `./scripts/verify.sh` must pass all 8 stages with 0 warnings.

---

### Task 1: Add New Architecture Contract Tests (RED Phase)

- [ ] **Step 1: Write expanded architecture contract tests in `tests/test_suite/test_architecture_contracts.rs`**
  - Add `test_organizer_does_not_depend_on_cli` asserting no file in `src/organizer/` imports `crate::cli`.
  - Add `test_subsystems_do_not_depend_on_orchestration_or_cli` asserting `db.rs`, `geo.rs`, `location/`, `classifier.rs`, `burst.rs` never import `organizer`, `catalog`, or `cli`.
- [ ] **Step 2: Run `cargo test --test test_suite test_architecture_contracts` and observe failure**
  - Must fail on `test_organizer_does_not_depend_on_cli` because `src/organizer/batch.rs`, `src/organizer/engine.rs`, and `src/organizer/mod.rs` import `crate::cli`.

---

### Task 2: Relocate Engine Arguments and Enums to Core Domain (GREEN Phase)

- [ ] **Step 1: Move `ReclassifyMode`, `OrganizeArgs`, `OrganizeAllArgs`, and `RenderCatalogArgs` to `src/types.rs`**
  - Ensure all fields and clap attributes are preserved.
  - Re-export them in `src/cli.rs` for backward compatibility.
- [ ] **Step 2: Update `src/organizer/` imports**
  - In `src/organizer/batch.rs`: change `use crate::cli::{...};` to `use crate::types::{...};`.
  - In `src/organizer/engine.rs`: change `use crate::cli::OrganizeArgs;` to `use crate::types::OrganizeArgs;` and `crate::types::ReclassifyMode`.
  - In `src/organizer/mod.rs`: change `&crate::cli::RenderCatalogArgs` to `&crate::types::RenderCatalogArgs`.
- [ ] **Step 3: Run `cargo test --test test_suite test_architecture_contracts` and verify GREEN**
  - Confirm all architecture contract tests now pass.

---

### Task 3: Comprehensive Verification & Cleanup (REFACTOR Phase)

- [ ] **Step 1: Run full test suite**
  - `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`
- [ ] **Step 2: Run full verification pipeline**
  - `./scripts/verify.sh` (all 8 stages)
- [ ] **Step 3: Commit changes**
  - Commit with message: `refactor(arch): decouple organizer from cli and enforce strict hexagonal architecture contracts`
