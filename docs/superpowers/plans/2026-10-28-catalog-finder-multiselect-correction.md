# Implementation Plan: Finder-Style Multi-Select & Series Photo Correction in Bird Catalog

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add macOS Finder-style multi-selection (Click to select single, Cmd/Ctrl+Click for disjoint selection, Shift+Click for contiguous range selection) to `bird_catalog.html` cards, and allow batch reclassifying all selected photos in a single modal action with unified addition to the bulk correction script.

**Architecture:**
- Extend the catalog HTML/CSS/JS template in `src/catalog/template.rs` with selection tracking (`selectedIds: Set<string>`, `lastSelectedId: string | null`).
- Render visual selection state on card elements (`.card.selected` with distinct macOS blue ring/accent overlay).
- Provide a persistent floating selection action bar when `>= 1` photos are selected, displaying: count of selected items, "Clear Selection" (`Esc`), and a prominent "Correct Selected (N)" button (`Cmd+K` / click).
- Enhance the correction modal to support both single-item and multi-item modes (`activeModalItems: Array<Item>`):
  - When multiple items are selected, show a thumbnail carousel / summary preview count.
  - Selecting a replacement species from the autocomplete search creates commands for all selected items.
  - "Add All to Batch Script" queues all selected items into `batchCommands` with duplicate avoidance.
- Maintain full source immutability, zero external runtime dependencies, and update the integration test suite in `tests/test_catalog.rs` to verify the JS multi-selection state machine and multi-bundle correction script generation via Node.js execution.

**Tech Stack:** Rust (avicull catalog generation), JavaScript (DOM events, selection range calculation, clipboard/download scripting), Node.js (headless JS unit/integration testing in cargo test).

**Spec:** User request: "Give an option to modify bird classification of a series of photos. I don't want to select them one by one. It should be the same experience as selecting icons in a finder. Cmd+click for disjoint multiple selections, and shift+click for contiguous selections."

## Global Constraints
- Pure standalone HTML with inline CSS/JS; zero external CDN or runtime server dependencies.
- Source immutability: generated correction bash scripts must target the APFS hard-link tree or in-place renames, never destructive to originals.
- TDD mandatory: every new behavior must have a failing test first.

---

### Task 1: Add Node.js/Rust Test Harness for Catalog Multi-Selection & Range Logic

**Files:**
- Modify: `tests/test_catalog.rs`

- [ ] **Step 1: Write failing integration test `test_catalog_finder_multiselect_and_bulk_correction_js`**
  - Verify range selection (Shift+Click) selects all visible items between `lastSelectedId` and clicked index in current filtered view.
  - Verify disjoint selection (Cmd/Ctrl+Click) toggles individual items without clearing the rest.
  - Verify plain click selects only that item and clears existing selection.
  - Verify that opening modal with 3 selected items and choosing a species generates correction blocks for all 3 items and adds them to `batchCommands`.
- [ ] **Step 2: Run test and observe failure**
  - Run `cargo test test_catalog_finder_multiselect --test test_catalog`
  - Ensure failure because selection functions and multi-item modal logic do not exist yet in template.rs.

### Task 2: Implement Finder-Style Card Selection UI & State Machine

**Files:**
- Modify: `src/catalog/template.rs`

- [ ] **Step 1: Add CSS styling for card selection and floating action bar**
  - Add `.card.selected` with border highlight, subtle blue background tint, and selection checkmark badge.
  - Add `.selection-bar` fixed bottom bar that slides up when `selectedIds.size > 0`.
- [ ] **Step 2: Implement selection event handling in catalog JavaScript**
  - Add `selectedIds = new Set()`, `lastSelectedId = null`.
  - Attach click handler to `.card` (ignoring clicks specifically on the direct "Correct Species" button or modal close):
    - If `e.metaKey || e.ctrlKey`: toggle item ID in `selectedIds`, update `lastSelectedId = itemId`.
    - Else if `e.shiftKey` and `lastSelectedId`: compute visible range between `lastSelectedId` and `itemId` in `getFilteredItems()`, add all to `selectedIds`.
    - Else: clear `selectedIds`, add `itemId`, set `lastSelectedId = itemId`.
  - Add keyboard navigation support:
    - `Escape`: clear selection.
    - `Cmd+A` / `Ctrl+A`: select all currently filtered cards.
- [ ] **Step 3: Add Floating Selection Bar**
  - Displays: "X photos selected", "Clear" button, and "Correct Selected (X)" button.

### Task 3: Support Multi-Item Correction in Modal & Batch Scripting

**Files:**
- Modify: `src/catalog/template.rs`

- [ ] **Step 1: Generalize modal state from `activeModalItem` to `activeModalItems`**
  - `openModal(itemIds)` accepting either a single ID or array/Set of IDs.
  - If single item: show current single photo thumbnail and current path.
  - If multiple items: show summary badge ("N photos selected"), thumbnail previews, and list of stems.
- [ ] **Step 2: Update `buildMoveCommands` and `addToBatch` to process all active modal items**
  - Generate concatenated or multi-target correction bash commands for each selected item.
  - "Add All to Batch Script" appends each item's correction to `batchCommands` (updating batch counter badge).
- [ ] **Step 3: Update direct "Correct Species" button on individual cards**
  - If clicking an individual card's "Correct Species" button while multiple cards are selected: prompt/default to correcting selected cards or select that card specifically.
- [ ] **Step 4: Re-run tests**
  - Run `cargo test test_catalog --test test_catalog` to confirm all tests pass green.

### Task 4: Full Verification and Linting

**Files:**
- All modified files

- [ ] **Step 1: Run format and clippy checks**
  - `cargo fmt --check`
  - `cargo clippy -- -D warnings`
- [ ] **Step 2: Run all catalog tests**
  - `cargo test test_catalog`
- [ ] **Step 3: Verify clean git status and commit**
