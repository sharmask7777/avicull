# Technical Specification: avicull — Local AI Bird Photo Organizer (macOS Apple Silicon)

## 1. Objective & Core Invariants
avicull is a local-first, deterministic command-line system written in Rust for macOS (Apple Silicon M-series) that projects bird photography archives from an immutable trip-based directory tree into a separate species-headed directory tree using POSIX hard links (`std::fs::hard_link`), naming every linked image and its associated sidecars `<OriginalStem>_<Confidence>.<ext>`.

The architecture combines the multi-stage detection and transactional manifest design of [SuperPicky](https://github.com/superpickyapp/Superpicky) with the visual + spatio-temporal prior scoring pipeline of [Cornell Merlin Bird ID](https://www.merlinbirds.org/about/).

### Non-Negotiable Filesystem Invariants
1. **Strict Source Immutability:** The input trip directory (`--input`) is strictly read-only. avicull must never move, rename, modify, or unlink any file inside `--input`.
2. **Separate Output Tree via Hard Links (`std::fs::hard_link`):** All organized files are materialized in a disjoint output directory (`--output`) on the same APFS volume using hard links. Both directory trees point to the exact same underlying APFS inodes, consuming zero additional disk space for image payloads.
3. **Upfront Volume & Disjointness Validation:** Before running inference, avicull must verify via `std::os::unix::fs::MetadataExt::dev()` that `--input` and `--output` reside on the same filesystem device (`st_dev`) so hard linking cannot fail mid-run with `EXDEV` (Cross-device link), and that `--output` is not nested inside `--input` (or vice versa).

### Target Transformation
**Input State (Immutable Trip-Based Directory Tree):**
```text
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

**Output State (Separate Hard-Linked Species Directory Tree):**
```text
/Volumes/Photos/Birds_By_Species/
├── Painted_Stork/
│   ├── DSC_4011_0.96.JPG # Hard link -> /Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/DSC_4011.JPG
│   ├── DSC_4011_0.96.xmp # Hard link -> /Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/DSC_4011.xmp
│   └── DSC_4012_0.94.JPG # Hard link -> /Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/DSC_4012.JPG
├── Streak-throated_Swallow/
│   └── DSC_4015_0.91.ARW # Hard link -> /Volumes/Photos/Trips/2025-11_Bharatpur_Keoladeo/DSC_4015.ARW
├── _Needs_Review/
│   └── Blyths_Reed_Warbler/
│       ├── IMG_9001_0.54.JPG # Hard link -> /Volumes/Photos/Trips/2026-01_Ranganathittu/IMG_9001.JPG
│       └── IMG_9001_0.54.XMP # Hard link -> /Volumes/Photos/Trips/2026-01_Ranganathittu/IMG_9001.XMP
├── _No_Bird_Detected/
│   └── 2026-01_Ranganathittu__IMG_9002_0.11.JPG
└── .avicull/
    └── manifest.sqlite
```

---

## 2. System Architecture: Rust Core + One-Time ONNX Export
To maximize compile-time safety, testability for AI coding agents, parallel I/O throughput, and native Apple Silicon hardware acceleration, the system splits into two components:
- **One-Time Model Exporter (`scripts/export_models.py`):** A minimal, deterministic Python script (managed via `uv`) that downloads pre-trained PyTorch weights and exports static `.onnx` graphs and precomputed species embeddings (`.safetensors`).
- **Production CLI & Engine (`avicull` Rust Workspace):** A standalone Rust binary that handles recursive directory traversal, sidecar bundling, EXIF/RAW parsing, CoreML/Metal ONNX inference, spatio-temporal prior weighting, burst consensus smoothing, SQLite transactional state tracking, and hard-link materialization.

| Subsystem | Crate / Tool | Role in Pipeline |
|---|---|---|
| ONNX Inference Engine | `ort` (coreml feature) | Executes detector and classifier `.onnx` models on Apple Neural Engine (ANE) and Metal GPU |
| Pure-Rust Tensor Ops | `candle` (metal feature) | Computes cosine similarity, temperature-scaled softmax, and burst embedding pooling |
| EXIF & GPS Parser | `nom-exif` & `kamadak-exif` | Extracts DateTimeOriginal, SubSecTimeOriginal, and GPS coordinates from JPEG, HEIC, and TIFF/RAW headers |
| RAW Preview Fallback | `exiftool` / embedded JPEG scanner | Extracts embedded full-size JPEG preview bytes from `.ARW`, `.CR3`, `.NEF`, `.ORF`, and `.RW2` files |
| State & Rollback DB | `rusqlite` (bundled feature) | Persists run manifests, inode numbers, predictions, burst assignments, and hard-link records in SQLite |
| Bird Detector Model | `Ultralytics YOLO11` (`yolo11m.onnx`) | Localizes bird bounding boxes (COCO class 14) to crop out background clutter |
| Visual Classifier | `BioCLIP 2.5 Huge` or `BioCLIP 2` | Computes fine-grained visual embeddings matched against global/regional avian taxonomy |
| Alternative Classifier | `OSEA MetaFGNet` | Fixed-class 10,000+ bird species classifier used by SuperPicky |
| Spatio-Temporal Prior | `SINR` (ICML 2023) | Computes $P(\text{species} \mid \text{lat}, \text{lon}, \text{week})$ to suppress out-of-range lookalikes |

---

## 3. Data Structures & Domain Contracts
All domain types must be strictly typed, serializable (`serde::Serialize`, `serde::Deserialize`), and decoupled from filesystem side effects so every stage can be unit-tested in isolation.

### 3.1. Asset Bundle (`AssetBundle`)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetBundle {
    pub bundle_id: String,                  // BLAKE3 hash of canonical primary_image path
    pub source_inode: u64,                  // APFS st_ino of primary_image for instant deduplication
    pub trip_folder_name: String,           // Immediate parent or root trip folder name
    pub original_stem: String,              // e.g., "DSC_4011"
    pub primary_image: PathBuf,             // Priority: RAW > JPEG > HEIC > PNG > TIFF
    pub companion_images: Vec<PathBuf>,     // e.g., ["DSC_4011.JPG"] when primary is "DSC_4011.ARW"
    pub sidecar_files: Vec<PathBuf>,        // e.g., ["DSC_4011.xmp", "DSC_4011.ARW.xmp"]
    pub capture_timestamp_ms: Option<i64>,
    pub gps_coords: Option<GpsCoordinate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GpsCoordinate {
    pub latitude: f64,
    pub longitude: f64,
    pub source: LocationSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationSource {
    ExifGps,
    SiblingPropagation,
    TripFolderGazetteer,
    UserDefaultRegion,
}
```

### 3.2. Classification & Confidence Contract (`BundleDecision`)
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesPrediction {
    pub taxon_id: String,                   // eBird / AviList taxon code (e.g., "paisto1")
    pub common_name: String,               // e.g., "Painted Stork"
    pub folder_slug: String,               // Sanitized: "Painted_Stork"
    pub scientific_name: String,           // e.g., "Mycteria leucocephala"
    pub visual_prob: f32,                  // Raw visual model probability [0.0, 1.0]
    pub geo_prior: f32,                    // Location/date occurrence prior [0.0, 1.0]
    pub combined_score: f32,               // Normalized posterior probability [0.0, 1.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleDecision {
    pub bundle_id: String,
    pub burst_id: Option<String>,
    pub detection_confidence: f32,
    pub bbox_xyxy_norm: Option<[f32; 4]>,
    pub top_predictions: Vec<SpeciesPrediction>, // Top-5 sorted descending
    pub frame_confidence: f32,             // Top-1 score before burst smoothing
    pub final_confidence: f32,             // Score written to hard-linked filename
    pub assigned_species_slug: String,
    pub routing_tier: RoutingTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingTier {
    ConfirmedSpecies,                      // final_confidence >= min_confidence (default 0.65)
    NeedsReview,                           // bird detected, but final_confidence < min_confidence
    NoBirdDetected,                        // detector confidence < min_detection_confidence (default 0.25)
}
```

---

## 4. Pipeline Stage Specifications

### Stage 1: Preflight Validation, Recursive Discovery & Sidecar Bundling (`src/scanner.rs`)
1. **Preflight Volume Check:**
   - Create `--output` directory if it does not exist.
   - Canonicalize `--input` and `--output` paths; reject with a descriptive error if either path is an ancestor of the other.
   - Compare `fs::metadata(&input)?.dev()` and `fs::metadata(&output)?.dev()`. If `st_dev` differs, abort immediately before running ML inference and report that hard links require `--input` and `--output` to reside on the same APFS volume.
2. **Recursive Scan:** Walk `input_dir` recursively using `walkdir`, ignoring hidden directories (`.avicull`, `.DS_Store`, `@eaDir`).
3. **Sidecar Grouping:** Group files in the same directory that share an identical filename stem (case-insensitive, stripping double extensions like `.ARW.xmp` back to stem `DSC_4011`).
   - Primary/Companion Images: `.jpg`, `.jpeg`, `.heic`, `.png`, `.tif`, `.tiff`, `.arw`, `.cr2`, `.cr3`, `.nef`, `.orf`, `.rw2`, `.dng`.
   - Sidecars: `.xmp`, `.dop`, `.pp3`, `.acr`, `.on1`.
4. **Fast Inference Decode Source:** When both a RAW file (`DSC_4011.ARW`) and a JPEG (`DSC_4011.JPG`) exist in the same bundle, decode the JPEG for fast inference while hard-linking all files in the bundle into the destination species folder. If only a RAW file exists, scan for the largest embedded JPEG preview stream (`0xFFD8..0xFFD9`) in memory.

### Stage 2: Location Resolution (`src/geo.rs`)
- **Priority 1 — EXIF GPS:** Parse `GPSLatitude` and `GPSLongitude` via `nom-exif`.
- **Priority 2 — Sibling EXIF Propagation:** If an image lacks GPS but another image in the same trip folder has valid EXIF GPS coordinates, inherit the median coordinates of that trip folder.
- **Priority 3 — Trip Folder Name Gazetteer:** Tokenize the trip folder path (e.g., `2025_11_Ranganathittu_Bird_Sanctuary` -> `["ranganathittu", "bird", "sanctuary"]`) and match against:
  - A user-editable `locations.toml` mapping table (`"ranganathittu" = { lat = 12.4244, lon = 76.6563 }`).
  - An embedded offline GeoNames / IBA (Important Bird Area) lookup table bundled with the binary.
- **Priority 4 — Fallback Region:** Use `--default-lat` and `--default-lon` from CLI config (or skip geo-prior weighting if unset).

### Stage 3: Bird Detection & Context-Padded Cropping (`src/detector.rs`)
- Run Ultralytics YOLO11 (`yolo11m.onnx`, input 640x640) via `ort` with `CoreMLExecutionProvider`.
- Filter detections to COCO class 14 (bird) with confidence $\ge \tau_{\text{det}}$ (default 0.25).
- If zero boxes pass $\tau_{\text{det}}$, mark bundle as `RoutingTier::NoBirdDetected` and skip Stage 4.
- If multiple bird boxes pass $\tau_{\text{det}}$, select the box maximizing $\text{confidence} \times \text{area}$ (prioritizing the primary foreground subject).
- Expand the selected bounding box by 15% on all sides (clamped to image boundaries), then crop and resize to the classifier's native resolution (224x224 or 336x336) using Lanczos3 resampling.

### Stage 4: Visual Classification + Spatio-Temporal Prior (`src/classifier.rs`)
- **Visual Embedding:** Pass the cropped bird tensor through the BioCLIP 2.5 Huge (or BioCLIP 2) ONNX vision encoder to obtain the L2-normalized image embedding $v \in \mathbb{R}^d$, along with temperature-scaled visual probabilities over the precomputed species text matrix $T \in \mathbb{R}^{N \times d}$:
  $$P_{\text{vis}}(s \mid I) = \text{softmax}(\tau \cdot v T^\top)_s$$
- **Geographical & Seasonal Prior:** If coordinates $(\text{lat}, \text{lon})$ are available, evaluate the exported SINR range estimation network to obtain presence probability $P_{\text{geo}}(s \mid \text{lat}, \text{lon}) \in [0, 1]$ for each species $s$. Apply a floor $\epsilon = 0.01$ so rare vagrants are down-weighted rather than hard-zeroed:
  $$P_{\text{post}}(s \mid I, \text{loc}) = \frac{P_{\text{vis}}(s \mid I) \cdot \max(P_{\text{geo}}(s \mid \text{loc}), \epsilon)^\alpha}{\sum_k P_{\text{vis}}(k \mid I) \cdot \max(P_{\text{geo}}(k \mid \text{loc}), \epsilon)^\alpha}$$
  where $\alpha \in [0.0, 2.0]$ is the configurable geo-prior strength (default 1.0).

### Stage 5: Burst Grouping & Consensus Smoothing (`src/burst.rs`)
- Sort all classified bundles within the same trip folder by `capture_timestamp_ms`.
- Group consecutive bundles into a `BurstCluster` when:
  - $|\Delta t| \le \text{burst\_window\_ms}$ (default 3000 ms), AND
  - Cosine similarity between their visual embeddings $v_i \cdot v_j \ge 0.80$ (ensuring the photographer did not pan to a different bird species mid-burst).
- Compute the burst-consensus posterior vector by confidence-weighted pooling across all $M$ frames in the burst:
  $$\bar{P}(s) = \frac{\sum_{m=1}^M w_m P_{\text{post}}(s \mid I_m, \text{loc})}{\sum_{m=1}^M w_m}, \quad w_m = \text{detection\_confidence}_m$$
- Assign every bundle in the burst to $\hat{s} = \text{argmax}_s \bar{P}(s)$ so a single burst is never scattered across multiple species folders.
- **Confidence Value Selection (`--confidence-mode`):**
  - `burst-consensus` (default): Writes $\bar{P}(\hat{s})$ as the confidence score.
  - `per-frame`: Writes $P_{\text{post}}(\hat{s} \mid I_m, \text{loc})$ for frame $m$ while still using $\hat{s}$ as the species folder.

### Stage 6: Hard-Link Materialization, Naming & Collision Resolution (`src/organizer.rs`)
- **Species Folder Sanitization:** Replace spaces with underscores, strip apostrophes, and replace non-alphanumeric characters (except `-` and `_`) with `_` (e.g., "Blyth's Reed Warbler" -> `Blyths_Reed_Warbler`).
- **Routing Rules (inside `--output`):**
  - If `routing_tier == ConfirmedSpecies` ($\text{final\_confidence} \ge 0.65$): Target directory is `<output_dir>/<Species_Slug>/`
  - If `routing_tier == NeedsReview` ($\text{final\_confidence} < 0.65$): Target directory is `<output_dir>/_Needs_Review/<Species_Slug>/`
  - If `routing_tier == NoBirdDetected`: Target directory is `<output_dir>/_No_Bird_Detected/`
- **Hard-Link Execution & Filename Construction:**
  - Format confidence to 2 decimal places (0.00 to 1.00): `<OriginalStem>_<Confidence:0.2>.<OriginalExt>` (e.g., `DSC_4011_0.96.JPG`, `DSC_4011_0.96.xmp`).
  - Preserve exact original extension casing (`.JPG` stays `.JPG`, `.xmp` stays `.xmp`).
  - For multi-extension sidecars (`DSC_4011.JPG.xmp`), insert the confidence suffix immediately after the stem (`DSC_4011_0.96.JPG.xmp`).
  - Create each destination entry using `std::fs::hard_link(&src_path, &dst_path)` so `st_ino` matches `src_path` and `st_nlink` increments by 1 without copying bytes.
- **Deterministic Collision & Re-Run Idempotency:**
  - Same Inode (`dst_meta.ino() == src_meta.ino()`): The exact file is already hard-linked at `dst_path` from an earlier run; treat as a no-op and mark `already_linked`.
  - Different Inode (Collision Across Trips): If `<TargetDir>/DSC_4011_0.96.JPG` already exists and points to a different inode, prefix the sanitized trip folder name: `<TargetDir>/<TripSlug>__DSC_4011_0.96.JPG` (and apply the identical `<TripSlug>__` prefix to all companion/sidecar hard links in that bundle).

---

## 5. SQLite Manifest & Clean Unlink Specification (`src/db.rs`)
All operations are logged in `<output_dir>/.avicull/manifest.sqlite` using `rusqlite`.
```sql
CREATE TABLE IF NOT EXISTS runs (
    run_id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    input_root TEXT NOT NULL,
    output_root TEXT NOT NULL,
    dry_run INTEGER NOT NULL,
    config_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    source_inode INTEGER NOT NULL,
    trip_folder TEXT NOT NULL,
    original_stem TEXT NOT NULL,
    capture_timestamp_ms INTEGER,
    lat REAL,
    lon REAL,
    location_source TEXT,
    burst_id TEXT,
    detection_conf REAL,
    bbox_json TEXT,
    assigned_species_slug TEXT NOT NULL,
    frame_conf REAL NOT NULL,
    final_conf REAL NOT NULL,
    routing_tier TEXT NOT NULL,
    top5_predictions_json TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE TABLE IF NOT EXISTS hardlink_operations (
    op_id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    bundle_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    dest_path TEXT NOT NULL,
    source_inode INTEGER NOT NULL,
    status TEXT NOT NULL, -- 'planned', 'linked', 'unlinked'
    executed_at TEXT,
    FOREIGN KEY(bundle_id) REFERENCES bundles(bundle_id)
);
```

---

## 6. CLI Interface Contract
```text
avicull <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
  scan       Verify APFS same-volume hardlink compatibility, sidecar bundles & location resolution
  organize   Run detection, classification, burst smoothing, and populate hard-linked species tree
  relink     Re-materialize hard links from cached SQLite predictions (e.g., with a new --min-confidence) without re-running ML inference
  clean      Unlink all generated hard links in --output for a given run_id (leaving --input untouched)
  report     Export a CSV/JSON summary of species counts and _Needs_Review items

OPTIONS FOR `avicull organize`:
  -i, --input <PATH>           Read-only root directory of trip-based bird photos
  -o, --output <PATH>          Separate target root directory (same APFS volume) for hard-linked species layout
  --dry-run                    Compute classifications and write SQLite plan without creating filesystem hard links
  --min-confidence <FLOAT>     Threshold for Confirmed vs _Needs_Review (default: 0.65)
  --min-detect <FLOAT>         YOLO11 bird detection confidence floor (default: 0.25)
  --burst-window-ms <INT>      Max timestamp gap for burst clustering in ms (default: 3000)
  --confidence-mode <MODE>     Filename score: [burst-consensus | per-frame] (default: burst-consensus)
  --geo-prior-weight <FLOAT>   Strength of SINR geographical prior [0.0..2.0] (default: 1.0)
  --locations-file <PATH>      Optional TOML file mapping trip folder keywords to {lat, lon}
  --models-dir <PATH>          Directory containing exported .onnx and .safetensors files
  --threads <INT>              Rayon worker thread count for I/O and preprocessing
```

---

## 7. Implementation Milestones & AI Agent Verification Harness
- **Milestone 1:** Workspace Scaffold, Volume Preflight, Scanner & Sidecar Bundler (`Cargo.toml`, `src/types.rs`, `src/scanner.rs`, `src/geo.rs`).
- **Milestone 2:** One-Time Python ONNX Model Exporter (`scripts/export_models.py`, `scripts/pyproject.toml`).
- **Milestone 3:** CoreML Inference Engine & Geo-Prior Math (`src/detector.rs`, `src/classifier.rs`).
- **Milestone 4:** Burst Consensus Engine (`src/burst.rs`).
- **Milestone 5:** Hard-Link Organizer, Instant Relink, SQLite Manifest & Clean CLI (`src/db.rs`, `src/organizer.rs`, `src/main.rs`).
