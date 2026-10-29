# avicull — Local AI Bird Photo Organizer (macOS Apple Silicon)

`avicull` is a local-first, deterministic command-line system written in Rust for macOS (Apple Silicon M-series) that organizes bird photography archives using local neural network detection (YOLO11) and classification (BioCLIP + Bayesian spatial-temporal priors across 11,167 species).

It supports two high-performance organization modes:
1. **In-Place Descriptive Renaming (Default)**: Renames image files and their associated sidecars (.xmp, .dop) directly in place to `<OriginalStem>_<Species_Common_Name>[_<Date>][_<Location>].<ext>` with zero file copying, zero data deletion, and generates a rich, standalone offline HTML photo catalog (`bird_catalog.html`).
2. **Hard-Linked Species Hierarchy (`--mode hierarchy`)**: Projects photos into a species-headed directory tree (`birds_hierarchy`) using POSIX zero-byte hard links (`std::fs::hard_link`), pointing to identical underlying inodes without consuming additional storage.

The architecture combines the multi-stage detection and transactional manifest design of [SuperPicky](https://github.com/superpickyapp/Superpicky) with the visual + spatio-temporal prior scoring pipeline of [Cornell Merlin Bird ID](https://www.merlinbirds.org/about/).

---

## Quick Install (Prebuilt macOS Apple Silicon Binary)

You can download and install the latest prebuilt binary directly from [GitHub Releases](https://github.com/sharmask7777/avicull/releases/latest) without needing a Rust compiler:

```bash
# Download latest binary tarball and checksum
curl -LO https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz.sha256

# Verify checksum
shasum -a 256 -c avicull-aarch64-apple-darwin.tar.gz.sha256

# Unpack and install
tar -xzf avicull-aarch64-apple-darwin.tar.gz
sudo cp avicull-aarch64-apple-darwin/avicull /usr/local/bin/

# Check installation
avicull --version
```

### One-Liner Install

```bash
curl -sL https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz | tar -xz && sudo cp avicull-aarch64-apple-darwin/avicull /usr/local/bin/
```

On first run, `avicull` will offer to automatically download and initialize the Cornell eBird 11,167-species taxonomy and neural model cache into `~/.cache/avicull/models`.

---

## Non-Negotiable Filesystem Invariants

1. **In-Place Descriptive Renaming (`--mode in-place`, Default)**:
   - Primary images, companion files, and sidecars (`.xmp`, `.dop`) are renamed in lockstep to `<OriginalStem>_<Species_Common_Name>[_<Date>][_<Location>].<ext>`.
   - Zero file copying and zero data deletion.
   - All renaming operations are transactionally journaled in `.avicull/manifest.sqlite`, guaranteeing lossless rollback via `avicull clean`.
   - Automatic collision resolution preserves unique filenames when bursts or duplicate names occur.
   - Manual user renames in Finder or terminal are automatically detected and reconciled on subsequent runs, renaming companions and sidecars in lockstep.
2. **Strict Source Immutability in Hierarchy Mode (`--mode hierarchy`)**:
   - The input trip directory (`--input`) is strictly read-only. `avicull` never moves, renames, modifies, or unlinks any file inside `--input`.
   - Files are projected into `<input>/birds_hierarchy` (or `--output`) using zero-byte POSIX hard links (`std::fs::hard_link`), pointing to identical underlying APFS inodes (`src_ino == dst_ino`) without consuming additional disk bytes.
3. **Multi-Level Coexistence (Hierarchy Mode)**:
   - You can create a `birds_hierarchy` in an individual trip folder (e.g. `Leh/birds_hierarchy`), and also in its parent directory (e.g. `Pictures/birds_hierarchy`). Both hierarchies co-exist seamlessly via hard links to the same underlying inodes (incrementing `st_nlink` to 2, 3, etc.) without duplicating storage.
4. **Auto-Detection & Incremental Updates**:
   - Running `avicull organize` on an existing archive automatically discovers newly added photos while reusing cached ML decisions from `.avicull/manifest.sqlite`. Only new or updated files undergo neural network inference.
5. **Upfront Volume & Safety Validation**:
   - In hierarchy mode, `avicull` verifies via `std::os::unix::fs::MetadataExt::dev()` that `--input` and `--output` reside on the same filesystem device (`st_dev`) so hard linking cannot fail mid-run with `EXDEV` (Cross-device link), and verifies that input is not nested inside output. Recursive scanners automatically skip `birds_hierarchy`, `.avicull`, `*.photoslibrary`, and system metadata files.
6. **Cross-Machine Portable Manifest & Durable User Overrides**:
   - The SQLite manifest—located at `<input>/.avicull/manifest.sqlite` (in-place mode) or `<output>/.avicull/manifest.sqlite` (hierarchy mode)—serves as a durable, self-contained record that travels with the archive.
   - When a photo directory or hierarchy tree is copied, rsynced, or mounted on another machine where inodes and absolute paths differ, `avicull organize` matches existing bundles by portable relative path and `(trip_folder, original_stem)` with 100% cache hits and zero redundant AI inferences.
   - User classification corrections—made via `avicull correct` (supporting burst propagation `--burst` / `-b`, query disambiguation, and fuzzy species autocorrect), manual Finder folder moves in hierarchy mode, manual in-place filename renames reconciled by `avicull organize`, or running the catalog's exported `apply_corrections.sh` script—are recorded as permanent overrides (`is_user_override = 1`, `final_confidence = 1.0`), permanently protected from neural re-classification or burst consensus smoothing across all routing tiers (`ConfirmedSpecies`, `NeedsReview`, or `NoBirdDetected`).
7. **Self-Contained Offline HTML Photo Catalog**:
   - Both modes automatically produce a standalone offline `bird_catalog.html` dashboard with zero external network dependencies, featuring embedded RAW previews, responsive search/filter, and one-click mislabel correction script generation.

---

## Filesystem Portability & Backup Nuances (UNIX, NTFS, & the exFAT Trap)

### 1. Inode Hard Link Extensibility to UNIX & Linux
The inode hard link mechanism in `avicull` relies on the standard POSIX `link(2)` system call (`std::fs::hard_link`). This is **not an APFS-specific feature**; it is universally supported by the Virtual Filesystem (VFS) layer of all POSIX-compliant UNIX and Linux operating systems:
- **Fully Supported Native Filesystems**: **ext4, Btrfs, XFS, ZFS, F2FS, UFS, and HFS+**. Directory entries map to an existing inode number (`st_ino`), incrementing `st_nlink` with 0 additional disk payload.
- **Network Storage (NAS)**:
  - **NFS (v3/v4)**: Supports hard links within the same export/mount point.
  - **SMB / CIFS**: Supported when backing Samba servers have UNIX extensions enabled.
- **The Invariant (`EXDEV`)**: A hard link can **never cross filesystem boundaries or mount points** (`EXDEV: Cross-device link`) because inode numbers are only unique within a specific filesystem instance (`st_dev`).

### 2. Copying to NTFS (Windows)
**NTFS natively supports hard links** (via the Master File Table / MFT record reference count, supporting up to 1,024 links per file):
- **The Copy Trap**: Dragging and dropping in macOS Finder or running a standard `cp -r` treats each link as an independent file and will **duplicate all payload bytes** on the target NTFS drive (e.g., 50 GB expands to 100 GB).
- **How to Copy Preserving Zero-Byte Hard Links**:
  Use `rsync` with the `-H` (`--hard-links`) flag:
  ```bash
  rsync -avH /Volumes/Photos/Leh /Volumes/NTFS_Drive/Photos/
  ```
  `rsync -H` tracks source inodes and reconstructs native hard links on the destination NTFS volume without duplicating bytes. Alternatively, pipe POSIX `tar`:
  ```bash
  tar -cf - -C /Volumes/Photos Leh | tar -xf - -C /Volumes/NTFS_Drive/Photos/
  ```
  Or run `avicull organize` directly against the mounted NTFS volume (using a writable NTFS driver like NTFS-3G or Paragon).

### 3. Crucial Warning for Photographers: The exFAT Trap
Many portable SSDs (SanDisk Extreme, Samsung T7, Crucial X9) and camera SD cards come formatted as **exFAT** out of the box for cross-platform plug-and-play compatibility.
- **exFAT does NOT support hard links** (no inode table or link reference counts).
- If you copy a `birds_hierarchy` to an exFAT drive, every hard link will inflate into a duplicate physical file, doubling disk consumption.
- **Rule of Thumb**:
  - Keep Mac drives formatted as **APFS**.
  - For Windows-compatible external drives with zero-byte links, format as **NTFS** and transfer with `rsync -avH`.
  - Alternatively, use the default **in-place renaming mode** (`avicull organize --input <DIR>`), which works on any filesystem including APFS, HFS+, NTFS, and exFAT without requiring hardlink support.

---

## Space-Efficient Cloud Storage & Network Share Processing

Photographers frequently store massive RAW photo libraries on cloud storage services (Google Drive, Proton Drive, Microsoft OneDrive, Dropbox, iCloud) or Network Attached Storage (NAS/SMB/NFS). 

### You Don't Need Your Photos Downloaded Locally!
> [!IMPORTANT]
> **Zero Local Disk Space Required Up Front**: Your cloud photos do **not** need to be downloaded or kept offline on your Mac before running `avicull`.
> - `avicull` processes your photos in **small, automatic batches** (default: `5GB` at a time).
> - For each batch, `avicull` automatically pulls only the photos it needs to read, extracts a tiny permanent thumbnail for your catalog, classifies the bird species, renames the file directly in your cloud drive, and **immediately frees up the local space** (evicting the file back to cloud-only) before moving to the next batch.
> - **Pre-existing local files remain untouched**: If you already marked certain favorite photos as "Keep Offline" or downloaded them previously, `avicull` detects this and will **never** delete or evict them.

---

### Step-by-Step: How to Find Your Cloud or Network Folder Path (No Technical Knowledge Needed)

If you are not comfortable using terminal paths, macOS provides two effortless ways to get the exact path to your photos:

#### Method 1: Drag-and-Drop from Finder (Easiest)
1. Open **Terminal** on your Mac.
2. Type `avicull organize --input ` (be sure to include the space after `--input`).
3. Open **Finder** and locate your photo folder inside **Google Drive**, **Proton Drive**, **OneDrive**, **Dropbox**, **iCloud Drive**, or your connected **Network Share**.
4. **Drag and drop** that folder directly into your Terminal window.
   - macOS will automatically type out the full, correct path for you (including all necessary quotes and spaces)!
5. Press **Enter**. That's it!

#### Method 2: Copy Pathname in Finder
1. Open **Finder** and navigate to your cloud or network photos folder.
2. Right-click (or hold `Control` and click) on the folder.
3. Hold down the **Option (⌥)** key on your keyboard.
   - Notice that *"Copy [Folder Name]"* changes into **"Copy \"[Folder Name]\" as Pathname"**!
4. Click **"Copy \"...\" as Pathname"**.
5. Switch to Terminal, type `avicull organize --input "`, press `Cmd + V` to paste, add a closing quote `"`, and press **Enter**:
   ```bash
   avicull organize --input "/Users/yourname/Library/CloudStorage/GoogleDrive-you@gmail.com/My Drive/Bird_Photos"
   ```

---

### Supported Platforms & Mounts
`avicull` automatically detects where your photos live without requiring any special flags or setup:
- **Cloud Storage (Apple FileProvider / CloudStorage)**:
  - Google Drive (`~/Library/CloudStorage/GoogleDrive-*`)
  - Proton Drive (`~/Library/CloudStorage/ProtonDrive-*`)
  - Microsoft OneDrive (`~/Library/CloudStorage/OneDrive-*`)
  - Dropbox (`~/Library/CloudStorage/Dropbox*` or `~/Dropbox*`)
  - Apple iCloud Drive (`~/Library/Mobile Documents/com~apple~CloudDocs`)
- **Network Shares (NAS)**:
  - Server Message Block (`SMB` / `CIFS` mounted under `/Volumes/*`)
  - Network File System (`NFS` mounted under `/Volumes/*`)

### Bounded Local Disk Cache Budget (`--max-cache-size`)
Cloud providers utilize dataless stub files (tombstones) that do not occupy physical disk blocks until read. Processing large archives without safeguards could inadvertently hydrate hundreds of gigabytes of RAW files, triggering local disk out-of-space crashes.

`avicull` solves this by chunking operations into bounded memory and disk working sets governed by `--max-cache-size` (default: `5GB`):
```bash
# Organize a cloud archive with a custom 2 GB cache budget
avicull organize --input ~/Library/CloudStorage/GoogleDrive-user@gmail.com/My\ Drive/Birds --max-cache-size 2GB
```
During each chunk:
1. Files within the budget are staged or hydrated.
2. AI bird detection and classification are executed.
3. Persistent thumbnails are saved to `.avicull/previews/` for instant browsing in `bird_catalog.html`.
4. In-place renames are safely executed.
5. Hydrated cloud stubs are evicted back to dataless state before the next chunk begins.

### Selective Eviction Guarantee
`avicull` maintains a strict locality invariant using macOS filesystem metadata (`SF_DATALESS` via `chflags(2)` / `getattrlist(2)`):
- **Pre-Existing Local Files Remain Local**: Any file that was already resident on disk prior to `avicull`'s run is **NEVER** evicted.
- **Downloaded Files Re-Evicted**: Only files that were explicitly dataless prior to `avicull` reading them and hydrated during the run are returned to cloud stubs.
- Your local offline availability preferences are preserved with zero disruption.

### Safe Eviction-Before-Rename Ordering
Cloud storage file providers (such as FileProvider framework on macOS) monitor file descriptors and inode state. Attempting to rename an in-flight hydrated file before its dirty buffers flush can trigger sync errors or macOS sync lockouts.
- `avicull` guarantees an **Eviction-Before-Rename** execution order:
  1. Primary image and sidecars are read and classified.
  2. Thumbnail preview is extracted into `.avicull/previews/`.
  3. Clean hydrated blocks are evicted back to dataless state (`SF_DATALESS`).
  4. In-place rename is applied cleanly to the cloud metadata stub.
- This prevents sync provider lockouts and avoids triggering redundant multi-gigabyte cloud re-uploads.

### Fast Local SSD Staging for SMB/NFS
Network file shares over Wi-Fi or 1GbE experience latency overhead when reading large embedded preview streams or RAW EXIF headers directly across the wire:
- For SMB/NFS mounts, `avicull` automatically leverages fast local SSD scratch space (`$TMPDIR/avicull_staging/`) to stage active chunks.
- RAW image previews and EXIF data are extracted at local NVMe speeds.
- Once processed, staging files are cleaned up immediately, maintaining 0 residual disk footprint.

### Network Failure Resilience & Delta Re-Runs
If a network disconnect, Wi-Fi drop, or cloud sync rate-limit occurs mid-run:
- `avicull` logs an informative notice and records transient failures in `.avicull/manifest.sqlite`.
- All completed bundles remain transactionally committed in SQLite and safely renamed.
- Simply re-running `avicull organize --input <PATH>` performs a fast delta re-run: it automatically picks up where it left off, bypassing previously organized bundles without re-downloading or re-inferring data.

### Hierarchy Mode Validation on Remote Backends
POSIX hard links (`std::fs::hard_link`) cannot span network boundaries or FileProvider virtual filesystems. `avicull organize` automatically validates the backend up front:
- If `--mode hierarchy` is attempted on a cloud storage or network share directory, `avicull` gracefully rejects the command with an explanatory error, directing you to use `--mode in-place` (the default).

---

## Target Transformation

### Input State (Camera Cards / Unorganized Trip Archive)
```
/Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/
├── DSC_4011.JPG
├── DSC_4011.xmp
├── DSC_4012.JPG
└── DSC_4015.ARW
```

### Transformation Option A: In-Place Descriptive Renaming (Default Mode)
In-place renaming updates file and sidecar stems directly in the source directory with zero byte copies and generates `bird_catalog.html`:
```
/Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/
├── DSC_4011_Painted_Stork_20251115_Bharatpur.JPG
├── DSC_4011_Painted_Stork_20251115_Bharatpur.xmp
├── DSC_4012_Painted_Stork_20251115_Bharatpur.JPG
├── DSC_4015_Streak-throated_Swallow_20251115_Bharatpur.ARW
├── bird_catalog.html
└── .avicull/
    ├── manifest.sqlite
    └── previews/
        └── DSC_4015.jpg
```

### Transformation Option B: Hard-Linked Species Directory Tree (`--mode hierarchy`)
Creates a separate species-headed tree using zero-byte POSIX hard links:
```
/Volumes/Photos/Trips/birds_hierarchy/
├── Painted_Stork/
│   ├── DSC_4011_2025-11-15_Bharatpur_0.96.JPG       # Hard link -> .../DSC_4011.JPG (same inode)
│   ├── DSC_4011_2025-11-15_Bharatpur_0.96.xmp       # Hard link -> .../DSC_4011.xmp (same inode)
│   └── DSC_4012_2025-11-15_Bharatpur_0.94.JPG       # Hard link -> .../DSC_4012.JPG (same inode)
├── Streak-throated_Swallow/
│   └── DSC_4015_2025-11-15_Bharatpur_0.91.ARW       # Hard link -> .../DSC_4015.ARW (same inode)
├── _Needs_Review/
│   └── Blyths_Reed_Warbler/
│       ├── IMG_9001_2026-01-10_Ranganathittu_0.54.JPG   # Hard link -> .../IMG_9001.JPG
│       └── IMG_9001_2026-01-10_Ranganathittu_0.54.XMP   # Hard link -> .../IMG_9001.XMP
├── _No_Bird_Detected/
│   └── 2026-01_Ranganathittu__IMG_9002_2026-01-10_Ranganathittu_0.11.JPG
├── bird_catalog.html
└── .avicull/
    ├── manifest.sqlite
    └── previews/
```

---

## System Architecture

The system splits cleanly into two decoupled layers:

```
Raw Archive Tree (--input)
        │
        ▼
Stage 1: Preflight Validation, Recursive Discovery & Sidecar Bundling
        │
        ▼
Stage 2: EXIF & Sibling Location Resolution (GPS / Trip Gazetteer)
        │
        ▼
Stage 3: Ultralytics YOLO11 Bird Detection & Clutter Crop (15% Context Pad)
        │
        ▼
Stage 4: BioCLIP 2 Visual Embedding + SINR Spatio-Temporal Prior Scoring
        │
        ▼
Stage 5: Burst Consensus Smoothing (Cosine Similarity Clustering)
        │
        ▼
Stage 6: SQLite Transactional State Tracking & Zero-Copy Hard Link Projection
        │
        ▼
Hard-Linked Species Tree (<input>/birds_hierarchy or --output)
```

1. **One-Time Model Exporter (`scripts/export_models.py`)**:
   - Python script (managed via `uv`) that exports YOLO11m detector ONNX models, BioCLIP 2 image encoder ONNX models, SINR spatio-temporal prior ONNX graphs, and precomputed species embedding `.safetensors`.
2. **Production Engine & CLI (`src/`)**:
   - Standalone Rust binary providing recursive traversal, RAW embedded preview extraction, CoreML/Metal ONNX execution, burst clustering, SQLite state management, and atomic hard-link projection.

---

## Installation & Build

### Prerequisites
- macOS on Apple Silicon (M1/M2/M3/M4)
- Rust 1.78+ (`cargo`)
- Python 3.10+ (for model export)

### Build Rust Binary
Optimized for Apple Silicon with thin LTO and unpacked debug symbols:
```bash
cargo build --release
```
The compiled binary will be available at `target/release/avicull`.

---

## Model Setup

To download weights and export the ONNX graphs:
```bash
cd scripts
uv run python export_models.py --output-dir ../models/
```
For testing or synthetic workflows, mock models can be generated instantly with:
```bash
python3 scripts/export_models.py --output-dir models/ --mock
```

---

## CLI Usage

### 1. `organize`: Full Pipeline Execution
Runs bird detection, classification, spatio-temporal prior weighting, burst smoothing, file organization, and generates an offline HTML catalog. Automatically detects and reconciles previous manual user adjustments (Finder moves in hierarchy mode or manual filename renames in in-place mode) into permanent SQLite overrides before processing new files.

#### Mode A: In-Place Descriptive File Renaming (Default)
By default, `avicull organize` renames files in place within `--input` and generates `<input>/bird_catalog.html`:
```bash
avicull organize --input /Volumes/Photos/Leh
# Renames files in-place: DSC_0042_House_Sparrow_20261025_Leh.CR2 + DSC_0042_...xmp
# Generates /Volumes/Photos/Leh/bird_catalog.html
```

#### Mode B: Hard-Linked Species Directory Tree (`--mode hierarchy`)
Organize files into `<input>/birds_hierarchy` using zero-byte POSIX hard links:
```bash
avicull organize --input /Volumes/Photos/Leh --mode hierarchy
# Creates /Volumes/Photos/Leh/birds_hierarchy/<Species_Common_Name>/...
# Generates /Volumes/Photos/Leh/birds_hierarchy/bird_catalog.html
```

You can also specify a custom output directory in hierarchy mode:
```bash
avicull organize \
  --input /Volumes/Photos/Leh \
  --output /Volumes/Photos/Leh_Avian_Custom \
  --mode hierarchy \
  --models-dir models \
  --min-confidence 0.65 \
  --min-detect 0.25 \
  --burst-window-ms 3000
```

Use `--dry-run` to preview operations without modifying files or creating links:
```bash
avicull organize --input /Volumes/Photos/Leh --dry-run
```

Use `--force` to re-run neural network detection and classification on all photos, bypassing cached predictions:
```bash
avicull organize --input /Volumes/Photos/Leh --force
```

### 2. Standalone Offline HTML Photo Catalog (`bird_catalog.html`)

Both in-place and hierarchy organization automatically produce a standalone offline `bird_catalog.html` dashboard located at `<input>/bird_catalog.html` (in in-place mode) or `<output>/bird_catalog.html` (in hierarchy mode).

Key capabilities include:
- **Zero External Dependencies**: Self-contained HTML, CSS, and vanilla JavaScript without any external CDNs or network calls. Functions seamlessly offline or in airplane mode in any modern browser.
- **Embedded RAW Thumbnail Extraction**: Automatically extracts high-resolution embedded preview JPEGs from camera RAW formats (`.CR2`, `.CR3`, `.ARW`, `.NEF`, `.DNG`) into `.avicull/previews/`, allowing instant browsing without loading multi-megabyte RAW payloads.
- **Dynamic Search & Tier Filtering**:
  - Filter by routing tier: **Confirmed Species** (green), **Needs Review** (amber), or **No Bird Detected** (slate).
  - Search photos in real time by common species name, scientific name, original filename, location, or capture date.
  - Interactive confidence threshold slider to visually preview routing tier boundaries.
- **Interactive Lightbox Modal**: Click any photo card to open a full-resolution lightbox view displaying camera EXIF exposure settings (shutter speed, aperture, ISO, focal length), hardware GPS coordinates, and top-5 ranked species predictions with confidence percentages.
- **Space-Efficient Hybrid Rendering**: Renders full-resolution images directly if files are locally resident on disk; seamlessly falls back to persistent thumbnails extracted into `.avicull/previews/` with an informative cloud status badge if files are evicted or offline.
- **Mislabel Correction & Batch Script Export**:
  - Click **Correct Species** on any photo card or within the lightbox.
  - Search the complete 11,167-species Cornell eBird taxonomy with instant keyboard autocomplete or enter a custom name.
  - The modal automatically displays the target filename update and synchronizes all companion files and sidecars (`.xmp`, `.dop`).
  - Click **Add to Batch Script** to queue correction commands.
  - Click **Download apply_corrections.sh** in the header to download a POSIX-compliant bash script powered by `avicull correct` that atomically renames your photo files, sidecars, and syncs the SQLite manifest.

### 3. `correct`: Manual Mislabel Corrections & Overrides

Correct a species classification or routing tier directly from the command line:
```bash
# Correct a single photo by stem or filename (in either hierarchy or in-place mode)
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "Painted Stork"

# Propagate correction across all frames in the same burst (alias: -b, --propagate-burst)
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "Painted Stork" --burst

# Disambiguate photos across multi-trip parent archives using relative paths:
avicull correct --input /Volumes/Photos --file "Leh/DSC_0042.CR2" --species "Painted Stork"

# Mark a false positive as non-bird, or mark an uncertain bird for review:
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "_No_Bird_Detected"
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "_Needs_Review/Mallard"
```
Actions coordinated safely in lockstep:
- Sanitizes the target species name (`Painted_Stork`) and assigns the appropriate routing tier (`ConfirmedSpecies`, `NeedsReview`, or `NoBirdDetected`).
- Updates `.avicull/manifest.sqlite` marking `is_user_override = 1` and `final_confidence = 1.0`.
- In **hierarchy mode**: moves/relinks primary images, companions, and sidecars into `<output>/<Species>/`, updating operation records and automatically pruning empty directories.
- In **in-place mode**: renames primary images, companions, and compound sidecars (`.xmp`, `.JPG.xmp`, `.dop`) in lockstep to reflect the new species while preserving original stems, dates, and locations.
- Automatically regenerates `bird_catalog.html` with updated counts and species tallies.

#### A. Burst Propagation (`--burst`, `-b`, `--propagate-burst`)
Bird photography frequently produces high-speed bursts of 3 to 20+ consecutive frames capturing the same subject. Reclassifying photos frame-by-frame is tedious and error-prone.
- **Single-Flag Propagation**: Passing `--burst` (or `-b`, `--propagate-burst`) reclassifies all frames in the burst that share the same `burst_id` within the trip. All primary images, companions, and sidecars are moved or renamed in lockstep and committed as durable overrides.
- **Interactive Prompting**: If you run `avicull correct` in an interactive terminal on a photo that belongs to a multi-photo burst without passing `--burst`, `avicull` detects the burst and prompts:
  ```text
  Photo is part of a 4-photo burst (burst_20261025_140201). Reclassify entire burst? [y/N]:
  ```
  Entering `y` automatically applies the correction to all burst frames.

#### B. Query Ambiguity Detection & Disambiguation
When organizing a parent archive with multiple trip subdirectories (e.g. `avicull correct --input /Volumes/Photos --file "DSC_0001"`), identical camera filenames may exist across different trips (e.g. `Leh/DSC_0001.JPG` and `Bharatpur/DSC_0001.JPG`).
- **Interactive Terminal**: `avicull` halts and presents a numbered selection menu with relative paths, trip names, capture dates, and current classifications:
  ```text
  Multiple photo bundles matched query 'DSC_0001':
    [1] Leh/DSC_0001.JPG (trip: Leh, date: 2026-10-25 09:12:30, current: House_Sparrow)
    [2] Bharatpur/DSC_0001.JPG (trip: Bharatpur, date: 2025-11-15 07:45:12, current: Painted_Stork)
  Select bundle to reclassify [1-2]:
  ```
- **Non-Interactive / Scripted Environments**: Fails with an actionable error listing all candidate paths, asking you to pass an unambiguous relative path (e.g. `--file "Leh/DSC_0001.JPG"`).

#### C. Fuzzy Species Autocorrect & Typo Suggestions
When entering a species name, typos or informal common names may occur (e.g. `--species "House Sparow"` or `--species "Pintail Duck"`).
- `avicull` compares the input against the official 11,167-species Cornell eBird taxonomy using fast character trigram similarity.
- **Interactive Terminal**: If a close taxonomy match is found (similarity $\ge 0.60$), `avicull` prompts:
  ```text
  Species 'House Sparow' not found in eBird taxonomy. Did you mean 'House Sparrow'? [Y/n]:
  ```
  Pressing Enter or `Y` uses the canonical eBird taxonomy name; answering `n` preserves your custom name.
- **Non-Interactive Mode**: Preserves your custom name without blocking, but logs an informative notice if an official eBird match exists.

#### D. Automatic Filesystem Reconciliation (`avicull organize`)
You are never locked into the CLI: manual adjustments made directly in macOS Finder or terminal are automatically recognized and reconciled on the next `avicull organize` run:
- **In-Place Mode Manual Renames**: If you manually rename a primary image file (e.g. renaming `DSC_0042_Mallard.JPG` to `DSC_0042_Northern_Pintail.JPG`), `avicull organize` decomposes the filename, matches the original stem (`DSC_0042`), detects the user edit, promotes it to a durable SQLite override (`is_user_override = 1`, `final_confidence = 1.0`), and automatically renames any companion files and sidecars (`DSC_0042_Mallard.xmp` -> `DSC_0042_Northern_Pintail.xmp`, `.dop`) in lockstep.
- **Hierarchy Mode Manual Directory Moves**: If you drag and drop a photo or burst in macOS Finder from `birds_hierarchy/_Needs_Review/Mallard/` to `birds_hierarchy/Northern_Pintail/` (or into `_No_Bird_Detected/`), running `avicull organize --mode hierarchy` automatically detects the move, reconciles the change into `.avicull/manifest.sqlite` as a durable user override, refreshes `bird_catalog.html`, and locks it against automated reclassification.

### 4. Multi-Level Coexistence & Incremental Runs (Hierarchy Mode)
You can organize both a specific trip folder and the parent archive:
```bash
# 1. Organize specific trip
avicull organize --input /Volumes/Photos/Leh --mode hierarchy
# Links created in /Volumes/Photos/Leh/birds_hierarchy (st_nlink = 2)

# 2. Organize parent archive
avicull organize --input /Volumes/Photos --mode hierarchy
# Links created in /Volumes/Photos/birds_hierarchy (st_nlink = 3)
```

When you add new photos to `/Volumes/Photos/Leh`, simply re-run:
```bash
avicull organize --input /Volumes/Photos/Leh
```
`avicull` detects already-cached photos from `.avicull/manifest.sqlite` and only runs inference on the newly added files.

### 5. `scan`: Preflight Validation & Discovery
Inspects photo bundles and sidecars without making changes:
```bash
avicull scan --input /Volumes/Photos/Leh
```

### 6. `report`: Query Run Manifest
Inspect classification results, routing tiers, and confidence scores:
```bash
# Using input directory
avicull report --input /Volumes/Photos/Leh --format table

# Or specifying explicit database path
avicull report --db /Volumes/Photos/Leh/.avicull/manifest.sqlite --format json
```

### 7. `relink`: Instant Threshold Adjustment
Adjust confidence thresholds (`--min-confidence`) without re-running neural network inference:
```bash
avicull relink --input /Volumes/Photos/Leh --min-confidence 0.80
```
This unlinks previous destination files and re-materializes links according to the new threshold instantly from SQLite state.

### 8. `clean`: Safe Rollback
Rolls back in-place renames to their original filenames or removes hard-linked files in the hierarchy directory:
```bash
# Clean latest run from input directory
avicull clean --input /Volumes/Photos/Leh

# Or cleanly roll back a specific run ID via database path
avicull clean --db /Volumes/Photos/Leh/.avicull/manifest.sqlite --run-id run-20261025-140000
```

### 9. `models`: Manage Decoupled ML & eBird Taxonomy Cache
Inspect cache status or refresh with the latest official Cornell eBird global taxonomy and BioCLIP embeddings without rebuilding the binary:
```bash
# Check status and age of local models cache
avicull models status

# Fetch latest eBird taxonomy and recompute embeddings into ~/.cache/avicull/models
avicull models update
```

### 10. `update`: Self-Update to Latest Release
Check for newer GitHub releases or self-update the running `avicull` executable:
```bash
# Check if an update is available without downloading
avicull update --check

# Upgrade to the latest release binary (with SHA-256 checksum verification)
avicull update

# If avicull is installed in a system directory like /usr/local/bin, run with sudo:
sudo avicull update
```
*Note: `avicull` automatically checks for newer releases in the background once every 24 hours. To disable background checks, export `AVICULL_NO_UPDATE_CHECK=1`.*

### 11. `location`: Granular Location Management & Priors

Assign GPS coordinates or an eBird gazetteer region to an entire trip, a specific subdirectory, or an individual photo:
```bash
# Set trip-level location by place name or coordinates
avicull location --dir "Leh" --location "Leh, Ladakh"
avicull location --dir "Leh" --location "34.1526, 77.5771"

# Set location for a specific subdirectory (e.g. within a parent archive)
avicull location --dir "pictures_bangalore/Turahalli forest Aug 2024" --location "Turahalli Forest, Bangalore"

# Set location override for an individual photo
avicull location --file "IMG_4001" --location "Bharatpur Bird Sanctuary"
```

#### Strict Location Precedence Invariant:
$$\text{Embedded EXIF GPS} > \text{Image Location Override} > \text{Subdirectory Override} > \text{Parent Directory Override}$$
- **Camera EXIF GPS is Strictly Immutable**: If a photo already has hardware GPS coordinates embedded in its EXIF metadata, `avicull` preserves those coordinates under all circumstances. Overrides are never applied over camera EXIF.
- **Hierarchical Subdirectory Matching**: Subdirectory locations match against the deepest matching subfolder first, falling back to parent folder locations.
- **Interactive Catalog Manager**: In `bird_catalog.html`, click the 📍 location badge or select photo ranges to update locations, re-run classifications, and review proposed changes in the interactive diff modal.

### 12. `help`: In-Terminal User Manual & Help Topics
Access interactive documentation and deep dives right from your terminal:
```bash
# View comprehensive user manual and commands
avicull help

# Deep-dive into specific topics or commands
avicull help quickstart
avicull help organize
avicull help scan
avicull help invariants
avicull help models
avicull help update
```

---

## Agent Guidelines & Continuous Model Updates

- **`AGENTS.md`**: Captures developer preferences, mandatory Test-Driven Development (TDD), multi-pronged testing strategies (unit, integration, synthetic E2E, real-world benchmarks), and multi-agent review crew verification loops.
- **`skills/avian-model-updater/SKILL.md`**: Agent skill instructing autonomous agents on how to continuously update the visual classifiers, species taxonomies, and spatio-temporal prior databases as newer models (e.g. BioCLIP 2.5, BioCLIP 3, SINR v2) are released.

---

## Testing & Verification

Run the Rust unit and filesystem integration test suite:
```bash
cargo test
```

Run the Python inference and taxonomy test suite:
```bash
uv run --with "torch,open-clip-torch,safetensors,ultralytics,pillow" python3 -m unittest discover -s tests_python
```

Run linter and formatting checks:
```bash
cargo fmt --check
cargo clippy -- -D warnings
```

---

## Global Location Inference & eBird Regional Priors

When organizing photos from DSLRs or mirrorless cameras lacking embedded GPS EXIF tags, `avicull` automatically infers geographic context from directory hierarchies without requiring manual tagging:

1. **Path Noise Stripping**: Automatically ignores holding directories (`raw`, `processed`, `selects`, `100canon`) and date patterns (`2025-04-19`, `aug 2024`, `oct 2025`) to extract candidate location phrases.
2. **Embedded Global Gazetteer**: Contains a curated offline registry of 100+ global birding hotspots (e.g. Saul Kere, Bharatpur, Serengeti, Everglades, Kruger, Monteverde) and world cities/regions, querying with fast in-memory trigram fuzzy matching.
3. **Interactive Disambiguation**: In interactive terminal sessions without GPS, `avicull` prompts with top 3–4 ranked candidates. Confirmed selections are cached in `.avicull/manifest.sqlite` so you are never re-prompted on re-runs.
4. **eBird Regional Checklist Prior**: Resolved locations map to regional administrative codes (`IN-KA` for Karnataka, `IN-LA` for Ladakh, `US-FL` for Florida, etc.). Species outside the official regional checklist receive an aggressive downweighting multiplier ($0.001$), instantly eliminating out-of-region hallucinations (such as Bald Eagles or Black Swans in South India) without neural spatial overhead.
5. **Headless / Batch Flags**:
   - `--location <NAME_OR_COORDS>` (or `-l`): Overrides or specifies the location name or decimal coordinates (`"12.97,77.59"`) for photos lacking GPS.
   - `--no-prompt`: Bypasses interactive prompts, auto-selecting top gazetteer candidates with confidence $> 0.85$ and proceeding unlocated otherwise.

---

## Continuous Integration & PR Sanity Checks

Every pull request targeting `main` is gated by the automated PR sanity check workflow in [`.github/workflows/pr_checks.yml`](.github/workflows/pr_checks.yml), running on macOS Apple Silicon (`macos-14`):

1. **Code Formatting & Clippy Linter**:
   - `cargo fmt --check`
   - `cargo clippy -- -D warnings`
2. **Rust Test Suite & Invariants**:
   - Runs all unit, filesystem integration, property-based, and E2E mock tests (`cargo test --locked`).
3. **Python & Gazetteer Integrity**:
   - Verifies the global gazetteer and eBird regional checklists (`pytest tests/test_build_gazetteer.py`).
   - Verifies Python ML inference scripts (`uv run python3 -m unittest discover -s tests_python`).
4. **Release Build Sanity**:
   - Verifies that `cargo build --release --locked` compiles with zero warnings or errors.

---

## Creating a New Release

### 1. Automated Releases on Merge to `main` (Recommended)
`avicull` uses continuous delivery via [`.github/workflows/release_on_merge.yml`](.github/workflows/release_on_merge.yml). Every time a pull request is merged into `main`:
1. The GitHub Action extracts the `version` from `Cargo.toml`.
2. It queries Git tags. If tag `v${VERSION}` already exists, it auto-increments the patch version (e.g., `0.2.0` -> `0.2.1`), updates `Cargo.toml` & `Cargo.lock`, and pushes a version bump commit to `main` with `[skip ci]`.
3. It compiles the release binary on macOS Apple Silicon (`macos-14`).
4. It packages `avicull-v${VERSION}-aarch64-apple-darwin.tar.gz` and calculates its SHA-256 checksum.
5. It publishes a new official GitHub Release with release notes and downloadable assets.

### 2. Manual Release Process
To manually build, package, and publish a new official GitHub release:

1. **Run Pre-Push Verification**:
   Ensure all 7 verification stages succeed:
   ```bash
   .githooks/pre-push
   ```

2. **Bump Version**:
   Update `version = "X.Y.Z"` in `Cargo.toml`.

3. **Compile Release Binary**:
   ```bash
   cargo build --release
   ```

4. **Package Distribution Tarball**:
   ```bash
   VERSION="0.2.0"
   DIST_NAME="avicull-v${VERSION}-aarch64-apple-darwin"
   mkdir -p "dist/${DIST_NAME}"
   cp target/release/avicull "dist/${DIST_NAME}/"
   cp LICENSE README.md "dist/${DIST_NAME}/"
   tar -czf "dist/${DIST_NAME}.tar.gz" -C dist "${DIST_NAME}"
   shasum -a 256 "dist/${DIST_NAME}.tar.gz" > "dist/${DIST_NAME}.tar.gz.sha256"
   ```

5. **Tag & Publish via GitHub CLI (`gh`)**:
   ```bash
   git tag -a "v${VERSION}" -m "Release v${VERSION}"
   git push origin "v${VERSION}"

   gh release create "v${VERSION}" \
     "dist/${DIST_NAME}.tar.gz" \
     "dist/${DIST_NAME}.tar.gz.sha256" \
     --title "v${VERSION}: Global Gazetteer & Location Inference" \
     --notes "Full release notes..."
   ```

---

## License
GNU General Public License v3.0 (GPL-3.0-or-later). See [LICENSE](LICENSE) for details.

