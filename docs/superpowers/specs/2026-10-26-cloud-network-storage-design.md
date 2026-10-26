# Design Document: Space-Efficient Cloud & Network Storage Processing

- **Date:** 2026-10-26
- **Status:** Proposed
- **Issue Reference:** [sharmask7777/avicull#6](https://github.com/sharmask7777/avicull/issues/6)
- **Author:** Antigravity & Pair Programming Partner

---

## 1. Intent & Problem Statement

Wildlife photographers regularly accumulate terabytes of high-resolution RAW captures across cloud storage providers (Google Drive, Proton Drive, Microsoft OneDrive, Dropbox, and iCloud Drive) or network-attached storage (SMB and NFS shares). 

Prior to this work, `avicull` assumed all photo payloads were locally resident on APFS with sufficient local disk capacity to scan and process the entire collection. Running `avicull` against a 2 TB remote or cloud collection would:
1. Trigger massive, uncontrolled downloading (hydration) of all RAW payloads at once, filling up the host Mac's internal SSD and causing out-of-disk crashes.
2. Produce broken or blank cards in `bird_catalog.html` whenever files are subsequently evicted to free disk space or when browsing offline.
3. Waste network bandwidth and time re-downloading previously classified photos on subsequent runs.
4. Risk accidental hard-link replication bloat or POSIX hard-link failures (`EXDEV`) across cloud sync engines and network shares.

This project introduces a **space-efficient chunked streaming engine** that bounds local disk consumption to a configurable budget (default: 5 GB), automatically detects macOS CloudStorage and network shares, materializes only what it needs, processes the delta, extracts persistent lightweight thumbnails, renames in place, and evicts only the files it explicitly downloaded.

Additionally, this design standardizes the catalog filename from `catalog.html` to `bird_catalog.html`.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Bounded Local Disk Budget (Default: 5 GB)**:
   - Chunked batch processing that materializes, processes, and immediately evicts batches to never exceed `--max-cache-size` (default `5GB`, configurable).
2. **Selective Eviction Invariant**:
   - Only evict files that were *explicitly downloaded* by `avicull`. Any files that were already resident on disk prior to `avicull`'s run must remain local.
3. **Automatic Storage Backend Detection**:
   - Inspect `--input` path to determine backend type: `MacOSCloudStorage` (Apple FileProvider stubs), `NetworkShare` (SMB/NFS mounts), or `LocalVolume` (standard APFS/HFS+).
   - Zero credential or API token setup; leverages native OS mount points and daemons.
4. **Dataless Pre-Scanning & Deferred EXIF Extraction**:
   - Pre-scan consumes 0 bytes of network download, using folder-level gazetteer matching and lightweight sidecars (`.xmp`) for location resolution. Camera EXIF and hardware GPS are extracted chunk-by-chunk only when a batch is materialized.
5. **Delta Processing via SQLite Manifest**:
   - On repeat runs, skip already classified files without re-downloading or hydrating their payloads.
6. **Space-Efficient, Fully Offline `bird_catalog.html`**:
   - Renders high-def original images if locally resident.
   - Gracefully falls back to persistent lightweight thumbnails (`.avicull/previews/<stem>.jpg`) with relative path and cloud status badge (☁️ Cloud / 💾 Local) when evicted or unmounted.
7. **Safe Eviction-Before-Rename Ordering**:
   - Evict clean hydrated blocks *prior* to in-place renaming (`evict(src) -> rename(src, dst)`) to prevent macOS `fileproviderd` sync lockouts.
8. **Network Failure Resilience**:
   - Retry failed downloads once with exponential backoff. Persistent failures are recorded in `manifest.sqlite` as pending and surfaced in a summary prompt to re-run the delta.
9. **Safe Correction Scripting**:
   - `apply_corrections.sh` and `avicull correct` rename dataless stubs atomically, and explicitly ensure eviction if any bytes were hydrated during the rename.
10. **Catalog Standardization**:
    - Rename `catalog.html` $\to$ `bird_catalog.html` across the codebase, CLI output, and documentation.

### Non-Goals
- Writing custom OAuth2 API clients for Google Drive, Proton Drive, or Dropbox. The system relies on native macOS FileProvider mount points and sync daemons.
- Direct RAW streaming over cloud REST APIs without local filesystem mounts.

### Anti-Goals
- **Never evict pre-existing local files**: If a user intentionally pinned or had local copies before running `avicull`, those files must not be converted into dataless stubs.
- **Never allow `--mode hierarchy` on Cloud or Network shares**: Hard links across filesystems (`EXDEV`) or cloud sync folders fail or duplicate multi-gigabyte files. Non-local backends strictly enforce `--mode in-place`.
- **Never store temporary staging on remote shares or cloud folders**: Temporary staging for network shares must reside on fast local SSD (`$TMPDIR/avicull_staging`).

---

## 3. Storage Backend Architecture

The storage layer is encapsulated behind an extensible trait:

```rust
pub enum StorageKind {
    MacOSCloudStorage, // Apple FileProvider (Google Drive, Proton Drive, OneDrive, iCloud)
    NetworkShare,      // SMB / NFS mounted under /Volumes/
    LocalVolume,       // APFS / HFS+ / Ext4 local disk
}

pub struct FileLocality {
    pub is_dataless: bool,        // True if file has SF_DATALESS flag or is remote
    pub pre_existing_local: bool, // True if file was already fully downloaded before avicull ran
    pub file_size_bytes: u64,
}

pub trait StorageBackend: Send + Sync {
    fn kind(&self) -> StorageKind;
    fn inspect_locality(&self, path: &Path) -> Result<FileLocality>;
    /// Returns the local path where bytes can be read (hydrated in-place or copied to local SSD staging).
    fn materialize(&self, path: &Path) -> Result<PathBuf>;
    /// Evicts hydrated blocks back to dataless or removes local staging copy.
    fn evict(&self, path: &Path) -> Result<()>;
    /// Renames the primary asset and sidecars.
    fn rename_asset(&self, src: &Path, dst: &Path) -> Result<()>;
}
```

### A. macOS CloudStorage Backend (`MacOSCloudStorage`)
- **Detection**: Paths under `~/Library/CloudStorage/` or paths where extended attributes contain `com.apple.fileprovider` / `com.apple.CloudDocs`.
- **Dataless Detection**: Query `stat()` structure flags:
  ```rust
  let is_dataless = (stat.st_flags & libc::SF_DATALESS) != 0;
  ```
  `SF_DATALESS` is `0x40000000` on Darwin. Reading metadata via `stat()` or `getattrlist()` does *not* hydrate the file.
- **Materialization**: Opening and reading the file automatically triggers macOS FileProvider hydration. Returns the original path (`PathBuf::from(path)`).
- **Eviction**: Invokes `fileproviderctl evict <path>` (or for iCloud `brctl evict <path>`). If `pre_existing_local == true`, eviction is skipped.
- **Eviction-Before-Rename**: Clean hydrated blocks are evicted *before* renaming. Renaming a dataless file in APFS is an instantaneous metadata-only operation that preserves the dataless state.

### B. Network Share Backend (`NetworkShare`)
- **Detection**: Filesystem type `smbfs` or `nfs` (via `statfs`).
- **Materialization**: Remote primary files stream over SMB/NFS into a local SSD scratch directory (`$TMPDIR/avicull_staging/`) up to the chunk budget. Returns the path to the staged copy on local SSD.
- **Processing**: Neural models run against the fast local SSD staging copy.
- **Rename & Cleanup**: Renames the remote target on the NAS and purges the local staging copy from `$TMPDIR/avicull_staging/`.
- **SQLite Concurrency**: Uses `PRAGMA journal_mode = TRUNCATE` and `PRAGMA locking_mode = NORMAL` for reliable operation over network shares.

### C. Local Volume Backend (`LocalVolume`)
- Standard APFS/HFS+ behavior. Returns original path; eviction is a no-op.

---

## 4. Chunked Processing Loop & Selective Eviction Lifecycle

```
Pre-Scan Input Tree (stat metadata only - 0 bytes downloaded)
                     │
                     ▼
Upfront Location Inference via Folder Gazetteer & .xmp Sidecars
                     │
                     ▼
Filter Delta against SQLite Manifest (skip already processed)
                     │
                     ▼
Partition into Chunks bounded by --max-cache-size (default: 5 GB)
                     │
                     ▼
  ┌────────────────────────────────────────────────────────┐
  │ For each Chunk:                                        │
  │   1. Record pre-existing locality for each file:       │
  │      was_dataless = (st_flags & SF_DATALESS != 0)      │
  │   2. Materialize/Stream with 1x retry on network error │
  │   3. Extract camera EXIF (capture time & GPS) from     │
  │      materialized files; smooth GPS with siblings      │
  │   4. Run YOLO11 bird detection                         │
  │   5. Run BioCLIP 2 species classification              │
  │   6. Generate persistent compressed preview:           │
  │      .avicull/previews/<original_stem>.jpg             │
  │   7. SELECTIVE EVICTION (Before Rename):               │
  │      For each file in chunk:                           │
  │        if was_dataless {                               │
  │            storage_backend.evict(original_path)        │
  │        }                                               │
  │   8. In-place rename primary + sidecars (metadata-only)│
  │   9. Update SQLite manifest state                      │
  └────────────────────────────────────────────────────────┘
                     │
                     ▼
Generate bird_catalog.html (with hybrid local/thumbnail rendering)
                     │
                     ▼
Summary Report (surfacing processed count, space saved, and retries)
```

---

## 5. Persistent Thumbnails & Space-Efficient `bird_catalog.html`

### Guaranteed Lightweight Previews
- For every processed photo (RAW or JPEG), `avicull` writes an optimized preview thumbnail to `.avicull/previews/<original_stem>.jpg`:
  - Width bounded to 800px max edge.
  - Quality set to JPEG 80 (~60–100 KB).
  - Stored inside `.avicull/previews/` which remains local and excluded from traversal scans.

### Hybrid Visual Rendering in `bird_catalog.html`
1. **Metadata attributes per photo card**:
   - `data-local-src`: Relative path to full-res asset.
   - `data-thumb-src`: `.avicull/previews/<original_stem>.jpg`.
   - `data-is-local`: Boolean indicating whether file is locally resident.
2. **Display Rules**:
   - If `is_local == true`, the `<img>` attempts to load `data-local-src`. If loading fails (e.g. evicted after generation), its `onerror` handler automatically swaps to `data-thumb-src` and adds a `[Cloud]` badge.
   - If `is_local == false`, the `<img>` loads `data-thumb-src` directly, displaying a sleek `☁️ Cloud` badge and relative path.
   - The interactive lightbox modal renders camera EXIF, GPS, classification confidence, and the thumbnail when offline.

---

## 6. Lightweight & Safe Correction Scripting

In `bird_catalog.html`, when the user requests species corrections:
- The generated `apply_corrections.sh` script handles cloud stubs cleanly:
  ```bash
  #!/usr/bin/env bash
  set -euo pipefail
  
  # Rename photo and sidecars (metadata-only on dataless APFS)
  mv "Leh/Day1/DSC_0042_OldSpecies.CR2" "Leh/Day1/DSC_0042_NewSpecies.CR2"
  [ -f "Leh/Day1/DSC_0042_OldSpecies.CR2.xmp" ] && mv "Leh/Day1/DSC_0042_OldSpecies.CR2.xmp" "Leh/Day1/DSC_0042_NewSpecies.CR2.xmp"
  
  # Ensure file remains evicted if cloud provider hydrated it during rename
  if command -v fileproviderctl >/dev/null 2>&1; then
      fileproviderctl evict "Leh/Day1/DSC_0042_NewSpecies.CR2" >/dev/null 2>&1 || true
  fi
  ```
- In `avicull correct` CLI subcommand:
  - If the target file was dataless prior to correction, `avicull correct` invokes `storage_backend.evict(new_path)` immediately after renaming, keeping disk usage zero.

---

## 7. Error Handling, Network Retries, and Delta Workflow

1. **Transient Network Interruptions**:
   - When hydrating a dataless file or reading over SMB/NFS, if an I/O error occurs (`std::io::ErrorKind::TimedOut`, `ConnectionReset`, etc.), `avicull` retries the download once after a 1.5-second backoff.
2. **Persistent Failures**:
   - If the second attempt fails, the bundle is marked `status = 'pending_download_failed'` in `manifest.sqlite` with the error message.
   - The chunk continues with remaining photos.
3. **Delta Completion Prompt**:
   - At the conclusion of `organize`, `avicull` outputs:
     ```text
     Batch complete: 485 photos organized, 3 failed due to cloud network timeout.
     Space saved via selective eviction: 18.4 GB.
     Run 'avicull organize --input <PATH>' to retry the 3 pending photos.
     ```

---

## 8. What's Left to the Builder

The builder has full discretion over:
1. Specific sub-chunk batch sizing heuristics (e.g. grouping by burst so burst smoothing has all frames in memory concurrently).
2. Internal image downscaling algorithm for thumbnail generation (`image::imageops::FilterType::Triangle` for maximum speed).
3. Exact SQLite schema columns for tracking `was_dataless` and `file_locality`.
4. Specific mock implementations in integration tests to simulate cloud stubs without requiring an active Google Drive account.
