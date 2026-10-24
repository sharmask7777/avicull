# avicull — Local AI Bird Photo Organizer (macOS Apple Silicon)

`avicull` is a local-first, deterministic command-line system written in Rust for macOS (Apple Silicon M-series) that projects bird photography archives from an immutable trip-based directory tree into a species-headed directory tree using POSIX hard links (`std::fs::hard_link`), naming every linked image and its associated sidecars `<OriginalStem>_<Confidence:0.2>.<ext>`.

The architecture combines the multi-stage detection and transactional manifest design of [SuperPicky](https://github.com/superpickyapp/Superpicky) with the visual + spatio-temporal prior scoring pipeline of [Cornell Merlin Bird ID](https://www.merlinbirds.org/about/).

---

## Non-Negotiable Filesystem Invariants

1. **Strict Source Immutability**: The input trip directory (`--input`) is strictly read-only. `avicull` never moves, renames, modifies, or unlinks any file inside `--input`.
2. **Separate Output Tree via Hard Links**: All organized files are materialized in a disjoint output directory (`--output`) on the same APFS volume using hard links (`std::fs::hard_link`). Both directory trees point to the exact same underlying APFS inodes, consuming zero additional disk space for image payloads.
3. **Upfront Volume & Disjointness Validation**: Before running inference, `avicull` verifies via `std::os::unix::fs::MetadataExt::dev()` that `--input` and `--output` reside on the same filesystem device (`st_dev`) so hard linking cannot fail mid-run with `EXDEV` (Cross-device link), and that `--output` is not nested inside `--input` (or vice versa).

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

### Output State (Separate Hard-Linked Species Directory Tree)
```
/Volumes/Photos/Birds_By_Species/
├── Painted_Stork/
│   ├── DSC_4011_0.96.JPG       # Hard link -> .../DSC_4011.JPG
│   ├── DSC_4011_0.96.xmp       # Hard link -> .../DSC_4011.xmp
│   └── DSC_4012_0.94.JPG       # Hard link -> .../DSC_4012.JPG
├── Streak-throated_Swallow/
│   └── DSC_4015_0.91.ARW       # Hard link -> .../DSC_4015.ARW
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
Hard-Linked Species Tree (--output)
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
- Python 3.10+ (for one-time model export)

### Build Rust Binary
```bash
cargo build --release
```
The compiled binary will be available at `target/release/avicull`.

---

## Model Export Setup

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

### 1. `scan`: Preflight Validation & Discovery
Validates device boundaries and inspects photo bundles and sidecars without making changes:
```bash
avicull scan --input /Volumes/Photos/Trips --output /Volumes/Photos/Birds_By_Species
```

### 2. `organize`: Full Pipeline Execution
Runs bird detection, classification, spatio-temporal prior weighting, burst smoothing, and materializes hard links:
```bash
avicull organize \
  --input /Volumes/Photos/Trips \
  --output /Volumes/Photos/Birds_By_Species \
  --models-dir models \
  --min-confidence 0.65 \
  --min-detect 0.25 \
  --burst-window-ms 3000
```
Use `--dry-run` to preview operations without creating filesystem links:
```bash
avicull organize --input /Volumes/Photos/Trips --output /Volumes/Photos/Birds_By_Species --dry-run
```

### 3. `report`: Query Run Manifest
Inspect classification results, routing tiers, and confidence scores:
```bash
avicull report --output /Volumes/Photos/Birds_By_Species --format json
avicull report --output /Volumes/Photos/Birds_By_Species --format table
```

### 4. `relink`: Instant Threshold Adjustment
Adjust confidence thresholds (`--min-confidence`) or fix routing without re-running multi-hour neural network inference:
```bash
avicull relink --output /Volumes/Photos/Birds_By_Species --min-confidence 0.80
```
This unlinks previous destination files and re-materializes links according to the new threshold instantly from SQLite state.

### 5. `clean`: Safe Rollback
Removes all hard-linked files and empty species directories in `--output`. The source photo archive in `--input` is completely untouched:
```bash
avicull clean --output /Volumes/Photos/Birds_By_Species
```

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
