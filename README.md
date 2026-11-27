# `avicull` — Local AI Photo Organizer for Birding & Wildlife Expeditions (macOS Apple Silicon)

`avicull` is a local-first command-line photo organizer written in Rust for macOS (Apple Silicon M-series). It organizes birding and wildlife photography archives using local neural detection (YOLO11) and multi-taxa classification (BioCLIP + Bayesian spatial-temporal priors across 11,167 Cornell eBird species and curated non-avian biodiversity).

It supports three organization modes:
1. **In-Place Non-Destructive Cataloging (Default, `--mode in-place`)**: Leaves all source photos and sidecars (`.xmp`, `.dop`) strictly untouched on disk, storing state in `<input>/avicull_catalog_files/` and generating a standalone offline HTML photo catalog (`catalog.html`).
2. **In-Place Descriptive Renaming (`--mode in-place-rename`)**: Transactionally renames primary files and companion sidecars in place to `<OriginalStem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location_Slug>].<ext>`, with lossless rollback via `avicull clean`.
3. **Hard-Linked Species Hierarchy (`--mode hierarchy`)**: Projects photos into `<input>/birds_hierarchy` (or `--output`) using zero-byte POSIX hard links (`std::fs::hard_link`) without duplicating image payloads.

For an interactive visual overview, open [docs/avicull_explainer.html](docs/avicull_explainer.html).

---

## Quick Install (Prebuilt macOS Apple Silicon Binary)

Download and install the latest prebuilt binary from [GitHub Releases](https://github.com/sharmask7777/avicull/releases/latest) without needing a Rust compiler:

```bash
# Download latest binary tarball and checksum
curl -LO https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz.sha256

# Verify checksum
shasum -a 256 -c avicull-aarch64-apple-darwin.tar.gz.sha256

# Unpack and install
tar -xzf avicull-aarch64-apple-darwin.tar.gz
sudo cp avicull-aarch64-apple-darwin/avicull /usr/local/bin/

# Verify installation
avicull --version
```

### One-Liner Install

```bash
curl -sL https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz | tar -xz && sudo cp avicull-aarch64-apple-darwin/avicull /usr/local/bin/
```

On first run, `avicull` offers to download and initialize the Cornell eBird 11,167-species taxonomy and neural model cache into `~/.cache/avicull/models`.

---

## Self-Hosting & Building from Source

If you prefer to build `avicull` yourself or host custom model weights:

### Prerequisites
- **macOS** on Apple Silicon (M1/M2/M3/M4)
- **Rust 1.80+** (`cargo`)
- **Python 3.10+** and `uv` (for model export)

### Build & Export Models
```bash
# 1. Build optimized release binary
cargo build --release
# Binary is at target/release/avicull

# 2. Export ONNX models and taxonomy embeddings into models/
cd scripts
uv run python export_models.py --output-dir ../models/
```

Or manage the decoupled model cache directly via the CLI:
```bash
avicull models status
avicull models update
```

---

## Photographic Categories

`avicull` routes captures into standardized singular categories:

| Category | Domain Scope | Example In-Place Filename | Example Hierarchy Path |
|---|---|---|---|
| **`Bird`** (Default) | 11,167 Cornell eBird species | `IMG_1001_Bird_House_Sparrow_20261025_Leh.CR2` | `birds_hierarchy/Bird/House_Sparrow/` |
| **`Mammalia`** | Mammals (big cats, primates, ungulates, etc.) | `IMG_1002_Mammalia_Snow_Leopard_20261025_Leh.CR2` | `birds_hierarchy/Mammalia/Snow_Leopard/` |
| **`Insecta`** | Insects, butterflies, dragonflies | `IMG_1003_Insecta_Monarch_Butterfly_20261025_Leh.CR2` | `birds_hierarchy/Insecta/Monarch_Butterfly/` |
| **`Reptilia`** | Reptiles (crocodiles, lizards, snakes, turtles) | `IMG_1004_Reptilia_Mugger_Crocodile_20261025_Ranthambore.CR2` | `birds_hierarchy/Reptilia/Mugger_Crocodile/` |
| **`Amphibia`** | Amphibians (frogs, toads, salamanders) | `IMG_1005_Amphibia_Common_Indian_Toad_20261025_Coorg.CR2` | `birds_hierarchy/Amphibia/Common_Indian_Toad/` |
| **`Scenery`** | Landscapes, mountain passes, valleys, skies | `IMG_1006_Scenery_Arid_Mountain_Valley_20261025_Leh.CR2` | `birds_hierarchy/Scenery/Arid_Mountain_Valley/` |
| **`Architecture`** | Built structures, monuments, temples | `IMG_1007_Architecture_Monumental_Architecture_20261025_Delhi.CR2` | `birds_hierarchy/Architecture/Monumental_Architecture/` |
| **`Portrait`** | Cultural street and field portraits | `IMG_1008_Portrait_Cultural_Street_Portrait_20261025_Leh.CR2` | `birds_hierarchy/Portrait/Cultural_Street_Portrait/` |
| **`Abstract`** | Textures, minimalist geometry, reflections | `IMG_1009_Abstract_Water_Ripples_Texture_20261025_Leh.CR2` | `birds_hierarchy/Abstract/Water_Ripples_Texture/` |
| **`_No_Subject_Detected`** | Zero-detection or empty frames | `IMG_1010_No_Subject_Detected_20261025_Leh.CR2` | `birds_hierarchy/_No_Subject_Detected/` |

---

## Safety Guarantees & Storage Behavior

- **Strict Source Immutability (Default `--mode in-place`)**: Original photos and sidecars are never moved, renamed, or deleted. State, previews, and SQLite manifests live inside `<input>/avicull_catalog_files/` (backward-compatible with `.avicull/`).
- **Zero-Byte Hard Links (`--mode hierarchy`)**: Projects organized trees on the same APFS/POSIX volume via hard links (`0` extra payload bytes) and supports simultaneous trip-level and parent-level hierarchies. Validates same-device (`st_dev`) and disjointness up front.
- **Portable Manifest & Durable Overrides**: `manifest.sqlite` travels with your archive across machines. Manual corrections (CLI, `catalog.html`, Finder moves, or manual renames) are stored as permanent user overrides (`is_user_override = 1`, `final_confidence = 1.0`).
- **Fast Software Re-Classification (`--reclassify-mode fast`)**: Threshold and regional prior updates re-score stored 512-d embeddings in milliseconds without re-running neural inference (`--reclassify-mode forced` opts into full re-inference).
- **Decoupled Offline Catalog (`catalog.html`)**: Renders in `<1.5s` from SQLite with zero external network calls. Single-trip catalogs ($\le 2,500$ photos) embed micro-thumbnails; large parent rollups ($50,000\text{--}100,000+$ photos) link to disk previews with lazy loading to keep HTML size `<30 MB`.

### Filesystem Portability & the exFAT Trap
- **UNIX / Linux / macOS**: Hard links work natively on APFS, HFS+, ext4, Btrfs, XFS, ZFS, and NFS/SMB (with UNIX extensions) within the same mount.
- **NTFS (Windows)**: Preserve hard links when copying via `rsync -avH` or POSIX `tar` (Finder drag-and-drop or `cp -r` duplicates payload bytes).
- **exFAT External SSDs**: exFAT does **not** support hard links. Copying `birds_hierarchy` to exFAT duplicates files. On exFAT drives, use the default `--mode in-place` instead of `--mode hierarchy`.

---

## Cloud Storage & NAS (Zero Local Space Up Front)

> [!IMPORTANT]
> You do **not** need to download cloud photos locally before running `avicull`. It processes archives in bounded batches (`--max-cache-size`, default `5GB`), extracts persistent previews, applies metadata/renames, and evicts only newly hydrated cloud stubs (`SF_DATALESS`) back to cloud-only. Pre-existing offline files are never evicted.

### Supported Mounts
- **Cloud Storage (macOS FileProvider)**: Google Drive, Proton Drive, OneDrive, Dropbox, iCloud Drive (`~/Library/CloudStorage/*` or `~/Library/Mobile Documents/...`).
- **Network Shares (NAS)**: SMB/CIFS and NFS under `/Volumes/*` (uses fast local SSD staging in `$TMPDIR/avicull_staging/` and `<20ms` directory-only probes).

### Finding Your Folder Path in Terminal
1. Type `avicull organize --input ` in Terminal (with a trailing space).
2. Drag and drop your cloud or NAS folder from Finder into Terminal, then press **Enter**.
   *(Or right-click the folder in Finder, hold **Option**, and click **Copy "..." as Pathname**.)*

---

## CLI Usage

### 1. `organize` — Organize a Trip Folder
```bash
# Default: In-place non-destructive cataloging
avicull organize --input /Volumes/Photos/Leh

# Opt-in: In-place descriptive file & sidecar renaming
avicull organize --input /Volumes/Photos/Leh --mode in-place-rename

# Opt-in: Zero-byte hard-linked species directory tree
avicull organize --input /Volumes/Photos/Leh --mode hierarchy

# Cloud archive with custom 2 GB cache budget
avicull organize --input ~/Library/CloudStorage/GoogleDrive-you@gmail.com/My\ Drive/Birds --max-cache-size 2GB

# Preview changes without writing
avicull organize --input /Volumes/Photos/Leh --dry-run

# Fast software re-scoring vs. forced neural re-inference
avicull organize --input /Volumes/Photos/Leh --reclassify-mode fast --min-confidence 0.85
avicull organize --input /Volumes/Photos/Leh --reclassify-mode forced
```

### 2. `organize-all` — Multi-Trip Batch Rollup
Organizes each immediate trip subdirectory and aggregates them into a root `catalog.html`:
```bash
# Incremental batch run (skips unchanged trips via <20ms directory probe)
avicull organize-all --input /Volumes/pictures

# Regenerate root rollup catalog with 0 image I/O
avicull organize-all --input /Volumes/pictures --view-only
```

### 3. `render-catalog` (alias `catalog`) — Instant HTML Regeneration
```bash
avicull render-catalog --input /Volumes/Photos/Leh
avicull catalog --input /Volumes/Photos/Leh --min-confidence 0.85 --reclassify-mode fast
```

### 4. `correct` — Manual Corrections & Burst Propagation
```bash
# Single photo (works in both in-place and hierarchy modes)
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "Painted Stork"

# Propagate across entire burst (-b / --burst / --propagate-burst)
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "Painted Stork" --burst

# Disambiguate across multi-trip archives
avicull correct --input /Volumes/Photos --file "Leh/DSC_0042.CR2" --species "Painted Stork"

# Route to special tiers
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "_No_Bird_Detected"
avicull correct --input /Volumes/Photos/Leh --file "DSC_0042" --species "_Needs_Review/Mallard"
```
Supports interactive burst prompting, ambiguity menus, fuzzy eBird autocorrect, and automatic reconciliation of manual Finder moves or renames on the next `avicull organize`.

### 5. `location` — Location Overrides & Regional Priors
When camera GPS EXIF is absent, `avicull` infers locations from folder names using an offline global gazetteer and eBird regional checklists (downweighting out-of-region species). Override explicitly when needed:
```bash
avicull location --dir "Leh" --location "Leh, Ladakh"
avicull location --dir "Leh" --location "34.1526, 77.5771"
avicull location --file "IMG_4001" --location "Bharatpur Bird Sanctuary"
```
Precedence: $\text{Embedded EXIF GPS} > \text{Image Override} > \text{Subdirectory Override} > \text{Parent Override}$.

### 6. Utility & Maintenance Commands
```bash
avicull scan --input /Volumes/Photos/Leh          # Preflight scan without changes
avicull report --input /Volumes/Photos/Leh        # Query manifest (table or --format json)
avicull relink --input /Volumes/Photos/Leh --min-confidence 0.80  # Instant threshold relink
avicull clean --input /Volumes/Photos/Leh         # Roll back renames or remove hierarchy links
avicull models status | update                    # Inspect or refresh eBird taxonomy & models
avicull update [--check]                          # Self-update binary (use sudo if in /usr/local/bin)
avicull help [topic]                              # Built-in manual (organize, invariants, models, ...)
```
*Tip: Set `AVICULL_NO_UPDATE_CHECK=1` to disable daily background release checks.*

---

## Contributing & Documentation

- **[CONTRIBUTING.md](CONTRIBUTING.md)** — Developer setup, TDD workflow, verification gates, CI checks, and release process
- **[CONCEPTS.md](CONCEPTS.md)** — Shared domain vocabulary and architectural concepts
- **[AGENTS.md](AGENTS.md)** — Concise operational guide for AI coding agents
- **[docs/README.md](docs/README.md)** — Index of designs, specs, plans, and engineering solutions

## License

GNU General Public License v3.0 (GPL-3.0-or-later). See [LICENSE](LICENSE).
