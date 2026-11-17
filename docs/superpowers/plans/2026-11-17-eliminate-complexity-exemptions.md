# Eliminate Cognitive Complexity Exemptions & Harden Maintainability Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the functions in the core codebase that currently rely on `#[allow(clippy::cognitive_complexity)]` exemptions (`geo.rs`, `reconcile.rs`, `correct.rs`) into cohesive, modular, deterministic helper functions, eliminating their complexity bypasses while keeping all behavioral invariants 100% green.

**Architecture:** Decompose multi-phase monolithic procedures into single-responsibility helpers:
1. `src/geo.rs::resolve_locations` (complexity 35 -> <=20): Extract parallel EXIF extraction, sibling median propagation, and gazetteer token matching.
2. `src/organizer/reconcile.rs::reconcile_inplace_modifications` (complexity 33 -> <=20): Extract file rename parsing, hash comparison, and journal recording.
3. `src/organizer/reconcile.rs::build_recovery_plan` (complexity 32 -> <=20): Extract journal entry validation, target destination conflict resolution, and plan action construction.
4. `src/organizer/correct.rs::apply_correction_batch` (complexity 66 -> <=25): Extract candidate matching, manifest updates, and file renaming/hardlink updates.

**Tech Stack:** Rust, Rayon, Clap, Clippy, cargo test.

## Global Constraints
- `unsafe_code = "deny"`
- Strict TDD: Verify unit & integration test suites before and after every single refactoring.
- Source immutability preserved across all operations.
- `./scripts/verify.sh` must pass all 8 stages with 0 warnings.

---

### Task 1: Refactor `src/geo.rs::resolve_locations` and Remove `#[allow(clippy::cognitive_complexity)]`
- [ ] **Step 1: Extract `extract_missing_exif_in_parallel` helper in `src/geo.rs`**
- [ ] **Step 2: Extract `compute_sibling_gps_medians` and `propagate_sibling_medians` helpers in `src/geo.rs`**
- [ ] **Step 3: Extract `resolve_bundle_locations_via_gazetteer` helper in `src/geo.rs`**
- [ ] **Step 4: Remove `#[allow(clippy::cognitive_complexity)]` from `resolve_locations`**
- [ ] **Step 5: Run `cargo test --test test_suite test_geo` and `cargo clippy --all-targets --all-features -- -D warnings` to verify green**

---

### Task 2: Refactor `src/organizer/reconcile.rs` and Remove Both `#[allow]` Exemptions
- [ ] **Step 1: Decompose `reconcile_inplace_modifications` into `detect_disk_modifications` and `apply_reconciliation_updates`**
- [ ] **Step 2: Remove `#[allow(clippy::cognitive_complexity)]` from `reconcile_inplace_modifications`**
- [ ] **Step 3: Decompose `build_recovery_plan` into `validate_journal_action` and `resolve_destination_conflict`**
- [ ] **Step 4: Remove `#[allow(clippy::cognitive_complexity)]` from `build_recovery_plan`**
- [ ] **Step 5: Run `cargo test --test test_suite test_inplace_rename` and clippy to verify green**

---

### Task 3: Refactor `src/organizer/correct.rs::apply_correction_batch`
- [ ] **Step 1: Extract `match_correction_bundle` helper**
- [ ] **Step 2: Extract `execute_single_correction` helper**
- [ ] **Step 3: Remove `#[allow(clippy::cognitive_complexity)]` from `apply_correction_batch`**
- [ ] **Step 4: Run `cargo test --test test_suite test_correct` and clippy to verify green**

---

### Task 4: Verify Unified Pipeline and Ship PR
- [ ] **Step 1: Run `./scripts/verify.sh` (all 8 stages)**
- [ ] **Step 2: Commit and push branch `feat/eliminate-complexity-exemptions`**
- [ ] **Step 3: Create PR and verify all 9 CI checks turn green**
