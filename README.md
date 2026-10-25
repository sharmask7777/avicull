# avicull — Local AI Bird Photo Organizer (macOS Apple Silicon)

`avicull` is a local-first, deterministic command-line system written in Rust for macOS (Apple Silicon M-series) that organizes bird photography archives using local neural network detection (YOLO11) and classification (BioCLIP + Bayesian spatial-temporal priors across 11,167 species).

It supports two high-performance organization modes:
1. **In-Place Descriptive Renaming (Default)**: Renames image files and their associated sidecars (.xmp, .dop) directly in place to `<OriginalStem>_<Species_Common_Name>[_<Date>][_<Location>].<ext>` with zero file copying, zero data deletion, and generates a rich, standalone offline HTML photo catalog (`catalog.html`).
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
2. **Strict Source Immutability in Hierarchy Mode (`--mode hierarchy`)**:
   - The input trip directory (`--input`) is strictly read-only. `avicull` never moves, renames, modifies, or unlinks any file inside `--input`.
   - Files are projected into `<input>/birds_hierarchy` (or `--output`) using zero-byte POSIX hard links (`std::fs::hard_link`), pointing to identical underlying APFS inodes (`src_ino == dst_ino`) without consuming additional disk bytes.
3. **Multi-Level Coexistence (Hierarchy Mode)**:
   - You can create a `birds_hierarchy` in an individual trip folder (e.g. `Leh/birds_hierarchy`), and also in its parent directory (e.g. `Pictures/birds_hierarchy`). Both hierarchies co-exist seamlessly via hard links to the same underlying inodes (incrementing `st_nlink` to 2, 3, etc.) without duplicating storage.
4. **Auto-Detection & Incremental Updates**:
   - Running `avicull organize` on an existing archive automatically discovers newly added photos while reusing cached ML decisions from `.avicull/manifest.sqlite`. Only new or updated files undergo neural network inference.
5. **Upfront Volume & Safety Validation**:
   - In hierarchy mode, `avicull` verifies via `std::os::unix::fs::MetadataExt::dev()` that `--input` and `--output` reside on the same filesystem device (`st_dev`) so hard linking cannot fail mid-run with `EXDEV` (Cross-device link), and verifies that input is not nested inside output. Recursive scanners automatically skip `birds_hierarchy`, `.avicull`, `*.photoslibrary`, and system metadata files.
6. **Self-Contained Offline HTML Photo Catalog**:
   - Both modes automatically produce a standalone offline `catalog.html` dashboard with zero external network dependencies, featuring embedded RAW previews, responsive search/filter, and one-click mislabel correction script generation.

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
In-place renaming updates file and sidecar stems directly in the source directory with zero byte copies and generates `catalog.html`:
```
/Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/
├── DSC_4011_Painted_Stork_20251115_Bharatpur.JPG
├── DSC_4011_Painted_Stork_20251115_Bharatpur.xmp
├── DSC_4012_Painted_Stork_20251115_Bharatpur.JPG
├── DSC_4015_Streak-throated_Swallow_20251115_Bharatpur.ARW
├── catalog.html
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
├── catalog.html
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
Runs bird detection, classification, spatio-temporal prior weighting, burst smoothing, file organization, and generates an offline HTML catalog.

#### Mode A: In-Place Descriptive File Renaming (Default)
By default, `avicull organize` renames files in place within `--input` and generates `<input>/catalog.html`:
```bash
avicull organize --input /Volumes/Photos/Leh
# Renames files in-place: DSC_0042_House_Sparrow_20261025_Leh.CR2 + DSC_0042_...xmp
# Generates /Volumes/Photos/Leh/catalog.html
```

#### Mode B: Hard-Linked Species Directory Tree (`--mode hierarchy`)
Organize files into `<input>/birds_hierarchy` using zero-byte POSIX hard links:
```bash
avicull organize --input /Volumes/Photos/Leh --mode hierarchy
# Creates /Volumes/Photos/Leh/birds_hierarchy/<Species_Common_Name>/...
# Generates /Volumes/Photos/Leh/birds_hierarchy/catalog.html
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

### 2. Standalone Offline HTML Photo Catalog (`catalog.html`)

Both in-place and hierarchy organization automatically produce a standalone offline `catalog.html` dashboard located at `<input>/catalog.html` (in in-place mode) or `<output>/catalog.html` (in hierarchy mode).

Key capabilities include:
- **Zero External Dependencies**: Self-contained HTML, CSS, and vanilla JavaScript without any external CDNs or network calls. Functions seamlessly offline or in airplane mode in any modern browser.
- **Embedded RAW Thumbnail Extraction**: Automatically extracts high-resolution embedded preview JPEGs from camera RAW formats (`.CR2`, `.CR3`, `.ARW`, `.NEF`, `.DNG`) into `.avicull/previews/`, allowing instant browsing without loading multi-megabyte RAW payloads.
- **Dynamic Search & Tier Filtering**:
  - Filter by routing tier: **Confirmed Species** (green), **Needs Review** (amber), or **No Bird Detected** (slate).
  - Search photos in real time by common species name, scientific name, original filename, location, or capture date.
  - Interactive confidence threshold slider to visually preview routing tier boundaries.
- **Interactive Lightbox Modal**: Click any photo card to open a full-resolution lightbox view displaying camera EXIF exposure settings (shutter speed, aperture, ISO, focal length), hardware GPS coordinates, and top-5 ranked species predictions with confidence percentages.
- **Mislabel Correction & Batch Script Export**:
  - Click **Correct Species** on any photo card or within the lightbox.
  - Search the complete 11,167-species Cornell eBird taxonomy with instant keyboard autocomplete or enter a custom name.
  - The modal automatically displays the target filename update and synchronizes all companion files and sidecars (`.xmp`, `.dop`).
  - Click **Add to Batch Script** to queue correction commands.
  - Click **Download apply_corrections.sh** in the header to download a POSIX-compliant bash script that atomically renames your photo files and sidecars.

### 3. Multi-Level Coexistence & Incremental Runs (Hierarchy Mode)
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

### 4. `scan`: Preflight Validation & Discovery
Inspects photo bundles and sidecars without making changes:
```bash
avicull scan --input /Volumes/Photos/Leh
```

### 5. `report`: Query Run Manifest
Inspect classification results, routing tiers, and confidence scores:
```bash
# Using input directory
avicull report --input /Volumes/Photos/Leh --format table

# Or specifying explicit database path
avicull report --db /Volumes/Photos/Leh/.avicull/manifest.sqlite --format json
```

### 6. `relink`: Instant Threshold Adjustment
Adjust confidence thresholds (`--min-confidence`) without re-running neural network inference:
```bash
avicull relink --input /Volumes/Photos/Leh --min-confidence 0.80
```
This unlinks previous destination files and re-materializes links according to the new threshold instantly from SQLite state.

### 7. `clean`: Safe Rollback
Rolls back in-place renames to their original filenames or removes hard-linked files in the hierarchy directory:
```bash
# Clean latest run from input directory
avicull clean --input /Volumes/Photos/Leh

# Or cleanly roll back a specific run ID via database path
avicull clean --db /Volumes/Photos/Leh/.avicull/manifest.sqlite --run-id run-20261025-140000
```

### 8. `models`: Manage Decoupled ML & eBird Taxonomy Cache
Inspect cache status or refresh with the latest official Cornell eBird global taxonomy and BioCLIP embeddings without rebuilding the binary:
```bash
# Check status and age of local models cache
avicull models status

# Fetch latest eBird taxonomy and recompute embeddings into ~/.cache/avicull/models
avicull models update
```

### 9. `update`: Self-Update to Latest Release
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

### 10. `help`: In-Terminal User Manual & Help Topics
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

