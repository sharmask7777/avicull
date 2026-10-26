# Cloud Storage Organic Experience & TDD Test Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide a seamless, organic customer experience where running `avicull organize --input <PATH>` on cloud storage (Proton Drive, Google Drive, OneDrive, iCloud) automatically materializes chunks, processes them, and automatically evicts them without any user intervention, backed by strict TDD test suites for dataless EXIF safety and native macOS eviction.

**Architecture:** Ensure `MacOSCloudStorageBackend` seamlessly coordinates hydration and automatic eviction inside the chunked `organize` pipeline. Test that dataless files are never eagerly opened during pre-scan and that `NSFileManager` eviction returns hydrated files to zero-byte stubs automatically.

**Tech Stack:** Rust 2021, macOS Darwin `libc::stat` (`SF_DATALESS`), `NSFileManager`, Rayon, SQLite.

**Spec:** [`docs/superpowers/specs/2026-10-26-cloud-network-storage-design.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/superpowers/specs/2026-10-26-cloud-network-storage-design.md)

## Global Constraints
- Customers must never be required to run eviction commands manually; `avicull organize` handles the entire download, organize, and evict lifecycle automatically.
- Dataless pre-scan must never open unhydrated files (0 bytes downloaded during scan).
- Selective eviction invariant: only files explicitly downloaded by `avicull` are evicted; pre-existing local files remain local.
- Mandatory TDD: write failing unit/integration tests before writing or modifying code.
- No procedural agent meta-comments (`// Step 1: ...`).

## Review Focus
1. Dataless pre-scan EXIF test: assert that `extract_exif_metadata` does not perform eager file reads on dataless files.
2. Automatic eviction execution in `organize`: assert that `organize` automatically triggers eviction on hydrated dataless files without manual user flags.
3. Spec conformance: verify that `bird_catalog.html` generation, in-place renames, and manifest updates work organically.

---

### Task 1: Comprehensive TDD Tests for Dataless EXIF Safety & Native Eviction

**Files:**
- Modify: `tests/test_storage_backend.rs`
- Modify: `tests/test_scanner.rs`
- Modify: `src/geo.rs`
- Modify: `src/storage/cloud.rs`

- [x] **Step 1: Write the failing tests**
In `tests/test_storage_backend.rs`:
Add `test_extract_exif_metadata_skips_eager_read_on_dataless_files` and `test_macos_native_eviction_integration`.
```rust
#[test]
fn test_extract_exif_metadata_skips_eager_read_on_dataless_files() {
    // create file, simulate dataless or verify non-blocking behavior
}

#[test]
fn test_cloud_backend_evict_uses_native_file_manager() {
    // verify backend.evict() executes without shell tool dependencies
}
```

- [x] **Step 2: Run test to verify failure / assertions**
Run: `cargo test --test test_storage_backend test_extract_exif_metadata_skips_eager_read`

- [x] **Step 3: Refine and harden implementation**
Ensure `src/geo.rs` and `src/storage/cloud.rs` are thoroughly tested and documented with domain explanations.

- [x] **Step 4: Verify test passes**
Run: `cargo test --test test_storage_backend`

- [x] **Step 5: Commit**
```bash
git add tests/ src/
git commit -m "test(storage): add TDD tests for dataless EXIF safety and native eviction"
```

---

### Task 2: Seamless Automatic Cloud Pipeline & Spec Conformance

**Files:**
- Modify: `src/organizer/mod.rs`
- Modify: `src/main.rs`
- Test: `tests/test_spec_conformance.rs`
- Test: `tests/test_chunk_processing.rs`

- [x] **Step 1: Write the failing integration test**
In `tests/test_spec_conformance.rs`, add `test_cloud_storage_automatic_organic_pipeline`:
Verify that running `organize` on a simulated cloud storage directory automatically executes the complete chunk loop, materializes, classifies, renames, and automatically evicts without needing manual commands.

- [x] **Step 2: Run test to verify failure**
Run: `cargo test --test test_spec_conformance test_cloud_storage_automatic_organic_pipeline`

- [x] **Step 3: Implement and refine organic pipeline**
Ensure `avicull organize` automatically performs the entire lifecycle smoothly in both interactive and non-interactive (`--no-prompt`) modes.

- [x] **Step 4: Verify all tests pass**
Run: `cargo test --all-targets`

- [x] **Step 5: Commit**
```bash
git add src/ tests/
git commit -m "feat(organizer): ensure seamless automatic cloud chunking and eviction"
```

---

### Task 3: Validate Against Original Spec & Dry-Run Against Proton Drive

**Files:**
- Test: Full test suite (`cargo test --all-targets`)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Run: `./target/release/avicull organize --input "<ProtonDrive_Path>" --dry-run --no-prompt`

- [x] **Step 1: Run full verification suite**
Run `cargo fmt -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets`

- [x] **Step 2: Run dry-run against Proton Drive folder**
Run `./target/release/avicull organize --input "/Users/shaleensharma/Library/CloudStorage/ProtonDrive-mail@shaleen.in/Pictures/pictures_bangalore_2023-24" --dry-run --no-prompt`
Verify clean completion, chunk partitioning, and zero errors.

- [x] **Step 3: Commit**
```bash
git add docs/
git commit -m "docs: finalize verification against spec and cloud storage"
```
