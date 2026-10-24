# avicull — Local AI Bird Photo Organizer (macOS Apple Silicon)

`avicull` is a local-first, deterministic command-line system written in Rust for macOS (Apple Silicon M-series) that projects bird photography archives from an immutable trip-based directory tree into a species-headed directory tree using POSIX hard links (`std::fs::hard_link`), naming every linked image and its associated sidecars `<OriginalStem>_<Confidence:0.2>.<ext>`.

The architecture combines the multi-stage detection and transactional manifest design of [SuperPicky](https://github.com/superpickyapp/Superpicky) with the visual + spatio-temporal prior scoring pipeline of [Cornell Merlin Bird ID](https://www.merlinbirds.org/about/).

---

## Non-Negotiable Filesystem Invariants

1. **Strict Source Immutability**: The input trip directory (`--input`) is strictly read-only. `avicull` never moves, renames, modifies, or unlinks any file inside `--input`.
2. **Default Output (`birds_hierarchy`) via Zero-Copy Hard Links**: By default, `avicull` materializes all organized files in `<input>/birds_hierarchy` (or any custom `--output` path) on the same APFS volume using hard links (`std::fs::hard_link`). Both directory trees point to the exact same underlying APFS inodes, consuming zero additional disk space for image payloads.
3. **Multi-Level Coexistence**: You can create a `birds_hierarchy` in an individual trip folder (e.g. `Leh/birds_hierarchy`), and also in its parent directory (e.g. `Pictures/birds_hierarchy`). Both hierarchies co-exist seamlessly via hard links to the same underlying inodes (incrementing `st_nlink` to 2, 3, etc.) without duplicating storage.
4. **Auto-Detection & Incremental Updates**: Running `avicull organize` on an existing hierarchy automatically discovers newly added photos while reusing cached ML decisions from `.avicull/manifest.sqlite`. Only new or updated files undergo neural network inference.
5. **Upfront Volume & Safety Validation**: Before running inference, `avicull` verifies via `std::os::unix::fs::MetadataExt::dev()` that `--input` and `--output` reside on the same filesystem device (`st_dev`) so hard linking cannot fail mid-run with `EXDEV` (Cross-device link), and verifies that input is not nested inside output. Recursive scanners automatically skip `birds_hierarchy`, `.avicull`, `*.photoslibrary`, and system metadata files.

---

## Target Transformation

### Input State (Immutable Trip-Based Directory Tree)
```
/Volumes/Photos/Trips/
├── 2025-11_Bharatpur_Keoladeo/
│   ├── DSC_4011.JPG
│   ├── DSC_4011.xmp
│   ├── DSC_4012.JPG
│   └── DSC_4015.ARW
└── 2026-01_Ranganathittu/
    ├── IMG_9001.JPG
    ├── IMG_9001.XMP
    └── IMG_9002.JPG
```

### Output State (Separate Hard-Linked Species Directory Tree: `birds_hierarchy`)
```
/Volumes/Photos/Trips/birds_hierarchy/
├── Painted_Stork/
│   ├── DSC_4011_0.96.JPG       # Hard link -> .../DSC_4011.JPG (same inode)
│   ├── DSC_4011_0.96.xmp       # Hard link -> .../DSC_4011.xmp (same inode)
│   └── DSC_4012_0.94.JPG       # Hard link -> .../DSC_4012.JPG (same inode)
├── Streak-throated_Swallow/
│   └── DSC_4015_0.91.ARW       # Hard link -> .../DSC_4015.ARW (same inode)
├── _Needs_Review/
│   └── Blyths_Reed_Warbler/
│       ├── IMG_9001_0.54.JPG   # Hard link -> .../IMG_9001.JPG
│       └── IMG_9001_0.54.XMP   # Hard link -> .../IMG_9001.XMP
├── _No_Bird_Detected/
│   └── 2026-01_Ranganathittu__IMG_9002_0.11.JPG
└── .avicull/
    └── manifest.sqlite
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
Runs bird detection, classification, spatio-temporal prior weighting, burst smoothing, and materializes hard links.

By default, the output directory defaults to `<input>/birds_hierarchy`:
```bash
avicull organize --input /Volumes/Photos/Leh
# Creates /Volumes/Photos/Leh/birds_hierarchy/
```

You can also specify a custom output directory:
```bash
avicull organize \
  --input /Volumes/Photos/Leh \
  --output /Volumes/Photos/Leh_Avian_Custom \
  --models-dir models \
  --min-confidence 0.65 \
  --min-detect 0.25 \
  --burst-window-ms 3000
```

Use `--dry-run` to preview operations without creating filesystem links:
```bash
avicull organize --input /Volumes/Photos/Leh --dry-run
```

### 2. Multi-Level Coexistence & Incremental Runs
You can organize both a specific trip folder and the parent archive:
```bash
# 1. Organize specific trip
avicull organize --input /Volumes/Photos/Leh
# Links created in /Volumes/Photos/Leh/birds_hierarchy (st_nlink = 2)

# 2. Organize parent archive
avicull organize --input /Volumes/Photos
# Links created in /Volumes/Photos/birds_hierarchy (st_nlink = 3)
```

When you add new photos to `/Volumes/Photos/Leh`, simply re-run:
```bash
avicull organize --input /Volumes/Photos/Leh
```
`avicull` detects already-cached photos from `.avicull/manifest.sqlite` and only runs inference on the newly added files.

### 3. `scan`: Preflight Validation & Discovery
Inspects photo bundles and sidecars without making changes:
```bash
avicull scan --input /Volumes/Photos/Leh
```

### 4. `report`: Query Run Manifest
Inspect classification results, routing tiers, and confidence scores:
```bash
# Using default birds_hierarchy
avicull report --input /Volumes/Photos/Leh --format table

# Or specifying explicit output path
avicull report --output /Volumes/Photos/Leh/birds_hierarchy --format json
```

### 5. `relink`: Instant Threshold Adjustment
Adjust confidence thresholds (`--min-confidence`) without re-running neural network inference:
```bash
avicull relink --input /Volumes/Photos/Leh --min-confidence 0.80
```
This unlinks previous destination files and re-materializes links according to the new threshold instantly from SQLite state.

### 6. `clean`: Safe Rollback
Removes all hard-linked files and empty species directories in the hierarchy directory. The source photo archive in `--input` is completely untouched:
```bash
avicull clean --input /Volumes/Photos/Leh
```

---

## Agent Guidelines & Continuous Model Updates

- **`AGENTS.md`**: Captures developer preferences, mandatory Test-Driven Development (TDD), multi-pronged testing strategies (unit, integration, synthetic E2E, real-world benchmarks), and multi-agent review crew verification loops.
- **`skills/avian-model-updater/SKILL.md`**: Agent skill instructing autonomous agents on how to continuously update the visual classifiers, species taxonomies, and spatio-temporal prior databases as newer models (e.g. BioCLIP 2.5, BioCLIP 3, SINR v2) are released.

---

## Testing & Verification

Run the comprehensive unit and integration test suite:
```bash
cargo test
```

Run linter and formatting checks:
```bash
cargo fmt --check
cargo clippy -- -D warnings
```

---

## License
MIT
