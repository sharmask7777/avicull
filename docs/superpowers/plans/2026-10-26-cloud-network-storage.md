# Space-Efficient Cloud & Network Storage Processing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable `avicull` to process multi-terabyte photo collections residing on cloud storage (Google Drive, Proton Drive, OneDrive, iCloud) and network shares (SMB/NFS) with a bounded local disk footprint (default: 5 GB), selective eviction of only explicitly downloaded files, dataless pre-scanning, hybrid rendering in `bird_catalog.html`, and network failure resilience.

**Architecture:** Introduce an extensible `StorageBackend` abstraction (`MacOSCloudStorage`, `NetworkShare`, `LocalVolume`) that detects filesystem mount types and dataless fault flags (`SF_DATALESS`). The pipeline streams unprocessed photos in bounded chunks (`--max-cache-size`), materializes payloads, extracts EXIF and compressed thumbnails, evicts clean hydrated blocks *prior* to in-place renaming, and generates `bird_catalog.html` with graceful thumbnail fallbacks.

**Tech Stack:** Rust 2021, macOS Darwin `libc::stat` (`SF_DATALESS`), `fileproviderctl`, Rayon, SQLite (`rusqlite`), `image` crate (Triangle downsampling for previews).

**Spec:** [`docs/superpowers/specs/2026-10-26-cloud-network-storage-design.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/superpowers/specs/2026-10-26-cloud-network-storage-design.md)

## Global Constraints
- Strictly enforce selective eviction: only evict files that had `is_dataless == true` before `avicull` hydrated them.
- Standardize catalog filename from `catalog.html` to `bird_catalog.html` everywhere.
- Strictly enforce `--mode in-place` on `MacOSCloudStorage` and `NetworkShare`; reject `--mode hierarchy` with a descriptive error.
- Network staging must reside on fast local SSD (`$TMPDIR/avicull_staging/`), never on remote shares.
- No procedural agent meta-comments (`// Step 1: ...`).
- Mandatory TDD: tests must fail first before implementing production code.

## Review Focus
1. Pre-existing local files must NOT be evicted: if a file has `SF_DATALESS == 0` prior to materialization, `evict()` must be a no-op.
2. Eviction must precede in-place rename: calling `evict(src)` then `rename(src, dst)` ensures macOS FileProvider does not reject eviction due to unsynced local rename changes.
3. Network share staging cleanup: transient copies in `$TMPDIR/avicull_staging` must be cleaned up on both success and failure paths without leaking disk space.
4. Transient network failure recovery: if a download fails with a connection error or timeout, retry once after backoff before marking as pending.
5. `bird_catalog.html` offline fallback: when a photo's full-res asset is not local or fails to load, `onerror` must swap to the compressed thumbnail without broken image icons.

---

### Task 1: Standardize Catalog Filename to `bird_catalog.html`

**Files:**
- Modify: `src/organizer/catalog.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/organizer/correct.rs`
- Modify: `src/main.rs`
- Modify: `tests/test_inplace_catalog.rs`
- Modify: `tests/test_reconcile_moves.rs`
- Modify: `tests/test_cli_correct.rs`

**Interfaces:**
- Consumes: existing `generate_html_catalog` function.
- Produces: `generate_html_catalog` writing to `bird_catalog.html`.

- [ ] **Step 1: Write the failing test**
Update `tests/test_inplace_catalog.rs` to assert that organizing creates `<dir>/bird_catalog.html` instead of `<dir>/catalog.html`.
```rust
#[test]
fn test_catalog_filename_is_bird_catalog() {
    let temp = tempfile::tempdir().unwrap();
    // organize bundles
    assert!(temp.path().join("bird_catalog.html").exists());
    assert!(!temp.path().join("catalog.html").exists());
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_inplace_catalog test_catalog_filename_is_bird_catalog`
Expected: FAIL (file `bird_catalog.html` does not exist).

- [ ] **Step 3: Update catalog generation and references**
In `src/organizer/catalog.rs`, `src/organizer/mod.rs`, `src/organizer/correct.rs`, `src/main.rs`, update default catalog file name from `catalog.html` to `bird_catalog.html`.

- [ ] **Step 4: Run all tests to verify they pass**
Run: `cargo test --all-targets`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/ tests/
git commit -m "refactor: rename catalog.html to bird_catalog.html"
```

---

### Task 2: Storage Backend Trait & Locality Detection

**Files:**
- Create: `src/storage/mod.rs`
- Modify: `src/lib.rs` (or `src/main.rs`)
- Test: `tests/test_storage_backend.rs`

**Interfaces:**
- Consumes: `std::path::Path`, Darwin `libc::stat`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum StorageKind {
      MacOSCloudStorage,
      NetworkShare,
      LocalVolume,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct FileLocality {
      pub is_dataless: bool,
      pub pre_existing_local: bool,
      pub file_size_bytes: u64,
  }

  pub trait StorageBackend: Send + Sync {
      fn kind(&self) -> StorageKind;
      fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality>;
      fn materialize(&self, path: &Path) -> std::io::Result<PathBuf>;
      fn evict(&self, path: &Path) -> std::io::Result<()>;
      fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()>;
  }

  pub fn detect_storage_backend(path: &Path) -> Box<dyn StorageBackend>;
  ```

- [ ] **Step 1: Write the failing test**
Create `tests/test_storage_backend.rs`:
```rust
use avicull::storage::{detect_storage_backend, StorageKind, FileLocality};

#[test]
fn test_local_volume_detection() {
    let temp = tempfile::tempdir().unwrap();
    let backend = detect_storage_backend(temp.path());
    assert_eq!(backend.kind(), StorageKind::LocalVolume);
    
    let file = temp.path().join("test.jpg");
    std::fs::write(&file, b"test").unwrap();
    let locality = backend.inspect_locality(&file).unwrap();
    assert!(!locality.is_dataless);
    assert!(locality.pre_existing_local);
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_storage_backend`
Expected: FAIL (module `storage` not found).

- [ ] **Step 3: Implement `StorageBackend` trait and `detect_storage_backend`**
In `src/storage/mod.rs`:
- Implement `detect_storage_backend` inspecting path prefixes (`~/Library/CloudStorage`) and mount types (`statfs` for `smbfs`/`nfs`).
- Implement Darwin `SF_DATALESS` check (`0x40000000`) on `st_flags` in `inspect_locality`.
- Wire `pub mod storage;` in `src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_storage_backend`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/storage/ src/lib.rs tests/test_storage_backend.rs
git commit -m "feat(storage): implement storage backend abstraction and locality detection"
```

---

### Task 3: Selective Eviction & Materialization Engine

**Files:**
- Modify: `src/storage/mod.rs`
- Create: `src/storage/cloud.rs`
- Create: `src/storage/network.rs`
- Create: `src/storage/local.rs`
- Test: `tests/test_storage_backend.rs`

**Interfaces:**
- Consumes: `StorageBackend`, `FileLocality`.
- Produces:
  - `MacOSCloudStorageBackend::evict`: executes `fileproviderctl evict` only when `pre_existing_local == false`.
  - `NetworkShareBackend::materialize`: streams to `$TMPDIR/avicull_staging/<uuid>_<stem>.<ext>`.
  - `NetworkShareBackend::evict`: deletes local staging file.

- [ ] **Step 1: Write the failing test**
In `tests/test_storage_backend.rs`:
```rust
#[test]
fn test_selective_eviction_skips_preexisting_local_files() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("local.jpg");
    std::fs::write(&file, b"local data").unwrap();

    let backend = avicull::storage::cloud::MacOSCloudStorageBackend::new();
    let locality = FileLocality {
        is_dataless: false,
        pre_existing_local: true,
        file_size_bytes: 10,
    };
    // Calling evict on a file marked pre_existing_local must be a no-op
    assert!(backend.evict_if_downloaded(&file, &locality).is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_storage_backend test_selective_eviction`
Expected: FAIL.

- [ ] **Step 3: Implement selective eviction and staging mechanics**
- In `src/storage/cloud.rs`, implement `evict_if_downloaded`: checks `locality.pre_existing_local`; if true, skips; if false, runs `fileproviderctl evict <path>`.
- In `src/storage/network.rs`, implement copy-to-staging in `$TMPDIR/avicull_staging/` with cleanup.
- Add retry helper with 1x backoff for transient I/O errors during `materialize`.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test test_storage_backend`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/storage/ tests/test_storage_backend.rs
git commit -m "feat(storage): implement selective eviction and network staging"
```

---

### Task 4: Chunked Processing Loop & `--max-cache-size` Budget

**Files:**
- Create: `src/organizer/chunk.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/test_chunk_processing.rs`

**Interfaces:**
- Consumes: `Vec<AssetBundle>`, `StorageBackend`, `--max-cache-size`.
- Produces:
  ```rust
  pub struct ChunkBudget {
      pub max_bytes: u64,
  }
  pub fn partition_into_chunks(bundles: Vec<AssetBundle>, max_bytes: u64) -> Vec<Vec<AssetBundle>>;
  ```

- [ ] **Step 1: Write the failing test**
Create `tests/test_chunk_processing.rs`:
```rust
#[test]
fn test_chunk_partitioning_respects_cache_budget() {
    // create mock bundles with known sizes (e.g. 50MB each)
    // partition with 120MB limit
    // verify no chunk exceeds 120MB (except single files larger than limit)
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_chunk_processing`
Expected: FAIL.

- [ ] **Step 3: Implement chunk partitioner and CLI option**
- In `src/cli.rs`, add `--max-cache-size <SIZE>` with default `"5GB"` (parsed to bytes via byte-unit parser).
- In `src/organizer/chunk.rs`, implement `partition_into_chunks` that groups bundles up to `max_bytes` while keeping bursts together where possible.
- In `src/organizer/mod.rs`, integrate chunk execution:
  - For each chunk:
    - Materialize primary images.
    - Extract camera EXIF and smooth GPS.
    - Run YOLO11 and BioCLIP 2.
    - Evict clean hydrated blocks (`backend.evict_if_downloaded`).
    - Rename primary and sidecars.
    - Update manifest.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test test_chunk_processing`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/chunk.rs src/organizer/mod.rs src/cli.rs src/main.rs tests/test_chunk_processing.rs
git commit -m "feat(organizer): implement chunked processing loop with cache budget"
```

---

### Task 5: Hybrid Offline Rendering in `bird_catalog.html` & Safe Correction Scripting

**Files:**
- Modify: `src/organizer/catalog.rs`
- Modify: `src/organizer/correct.rs`
- Test: `tests/test_inplace_catalog.rs`
- Test: `tests/test_cli_correct.rs`

**Interfaces:**
- Consumes: `AssetBundle`, `FileLocality`.
- Produces:
  - Guaranteed thumbnail generation at `.avicull/previews/<stem>.jpg` for all processed photos.
  - Hybrid card HTML with `data-local-src`, `data-thumb-src`, `data-is-local`, and `onerror` fallback.
  - `apply_corrections.sh` with `fileproviderctl evict` safeguarding.

- [ ] **Step 1: Write the failing test**
In `tests/test_inplace_catalog.rs`:
```rust
#[test]
fn test_catalog_hybrid_rendering_and_thumbnail_fallback() {
    // verify html contains data-thumb-src pointing to .avicull/previews/
    // verify html contains onerror fallback handler
    // verify apply_corrections.sh includes eviction safeguard
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_inplace_catalog test_catalog_hybrid_rendering`
Expected: FAIL.

- [ ] **Step 3: Implement hybrid rendering & correction script eviction**
- In `src/organizer/catalog.rs`:
  - Ensure `.avicull/previews/<stem>.jpg` is written for both RAW and JPEG assets.
  - Render `data-local-src` and `data-thumb-src` on each photo card.
  - Add JavaScript `onerror="this.onerror=null; this.src=this.dataset.thumbSrc; this.closest('.card').classList.add('evicted');"`.
  - In `render_correction_script_template`, append `fileproviderctl evict` check for renamed files.
- In `src/organizer/correct.rs`:
  - If file was dataless prior to correction, invoke eviction on the renamed file.

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test test_inplace_catalog --test test_cli_correct`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/organizer/catalog.rs src/organizer/correct.rs tests/
git commit -m "feat(catalog): add hybrid thumbnail rendering and correction script eviction"
```

---

### Task 6: Error Resilience, Manifest State & Summary Reporting

**Files:**
- Modify: `src/db.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/main.rs`
- Test: `tests/test_cloud_resilience.rs`

**Interfaces:**
- Consumes: SQLite schema, chunk execution results.
- Produces:
  - `was_dataless` column in `bundles` table.
  - `status = 'pending_download_failed'` for persistent network timeouts.
  - End-of-run summary with space saved metrics and delta retry suggestion.

- [ ] **Step 1: Write the failing test**
Create `tests/test_cloud_resilience.rs`:
```rust
#[test]
fn test_transient_failure_records_pending_and_resumes() {
    // simulate download failure on 1 file in a 5-file batch
    // verify 4 files succeed, 1 is marked pending_download_failed
    // verify summary counts 4 succeeded, 1 pending
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_cloud_resilience`
Expected: FAIL.

- [ ] **Step 3: Implement manifest tracking and summary metrics**
- In `src/db.rs`, add `was_dataless INTEGER DEFAULT 0` column to `bundles` table schema.
- In `src/organizer/mod.rs`, record failed downloads as `pending_download_failed` and track `space_saved_bytes`.
- In `src/main.rs`, print the cloud processing summary:
  ```text
  Cloud batch complete: 485 photos organized, 3 pending retry.
  Disk space saved via selective eviction: 18.4 GB.
  ```

- [ ] **Step 4: Run tests to verify they pass**
Run: `cargo test --test test_cloud_resilience`
Expected: PASS.

- [ ] **Step 5: Commit**
```bash
git add src/db.rs src/organizer/mod.rs src/main.rs tests/test_cloud_resilience.rs
git commit -m "feat(storage): track dataless state and report space savings"
```

---

### Task 7: Documentation & Full Pipeline Verification

**Files:**
- Modify: `README.md`
- Test: All tests (`cargo test --all-targets`)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt -- --check`

- [ ] **Step 1: Update README.md**
Document:
- Cloud storage support (Google Drive, Proton Drive, OneDrive, iCloud) and network share (SMB/NFS) support.
- `--max-cache-size` option (default: 5 GB).
- Selective eviction guarantee (pre-existing local files remain local).
- `bird_catalog.html` hybrid local/thumbnail rendering.
- Offline and delta workflows.

- [ ] **Step 2: Run full verification suite**
Run:
```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```
Expected: All formatting clean, 0 clippy warnings, 100% tests passing.

- [ ] **Step 3: Commit**
```bash
git add README.md
git commit -m "docs: document cloud storage processing and bird_catalog.html"
```
