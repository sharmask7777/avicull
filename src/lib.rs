//! # `avicull` — High-Performance Zero-Byte Avian Photo Culling & Hierarchy Organizer
//!
//! `avicull` is an intelligent, high-performance photo organization engine specifically architected
//! for avian wildlife photographers handling massive photo collections, bursts, and raw camera assets.
//!
//! ## Architectural Invariants & Guarantees
//!
//! 1. **Strict Source Immutability**:
//!    The input photo archive (`--input`) is strictly read-only. `avicull` never moves, renames,
//!    modifies, or unlinks any file inside the source archive during hierarchy generation.
//!
//! 2. **Zero-Byte Output Tree via APFS Hard Links**:
//!    Organized directory hierarchies (`Taxonomy / Species / Date_Location`) reference the identical
//!    underlying file inodes on macOS APFS (`dst_meta.ino() == src_ino`) using POSIX hard links
//!    (`std::fs::hard_link`). This consumes 0 additional disk bytes for multi-gigabyte RAW image payloads.
//!
//! 3. **Parent & Child Multi-Level Coexistence**:
//!    A trip subdirectory hierarchy (e.g. `Pictures/Leh/birds_hierarchy`) and its parent folder
//!    (e.g. `Pictures/birds_hierarchy`) coexist seamlessly without data duplication or clobbering,
//!    cleanly incrementing filesystem inode link counts (`st_nlink`).
//!
//! 4. **Exclusion Invariants During Traversal**:
//!    Directory traversal strictly skips output directories (`birds_hierarchy`), internal manifests
//!    (`.avicull`), Apple Photos packages (`*.photoslibrary`), NAS metadata (`@eaDir`), and hidden
//!    system files (`.*`).
//!
//! 5. **Two-Phase Atomic Operations**:
//!    Filesystem operations (such as in-place renames, delta updates, or reclassification) record
//!    pre- and post-state in an embedded SQLite manifest before modifying disk state, guaranteeing
//!    safe atomic rollbacks.
//!
//! ## Core Architecture
//!
//! The pipeline is split into modular stages:
//! - [`scanner`]: Traverses source trees, filters paths, groups companion formats (RAW + JPEG) and sidecars (`.xmp`).
//! - [`detector`]: Runs YOLO bird detection to crop bounding boxes with square aspect-ratio padding.
//! - [`classifier`]: Executes species inference, applying Bayesian spatial-temporal priors and logit temperature scaling.
//! - [`burst`]: Clusters consecutive shots into bursts and calculates confidence-weighted consensus pooling.
//! - [`location`]: Cascades EXIF GPS, nearest gazetteer waypoints, directory heuristics, and trigram fuzzy matching.
//! - [`organizer`]: Materializes APFS zero-byte hardlink trees or performs atomic in-place renaming.
//! - [`catalog`]: Produces a standalone, interactive, zero-dependency HTML5 review and curation dashboard.
//! - [`db`]: Embeds SQLite manifest storage for sub-second incremental delta re-runs.

pub mod burst;
pub mod catalog;
pub mod classifier;
pub mod cli;
pub mod dataset;
pub mod db;
pub mod detector;
pub mod geo;
pub mod help;
pub mod location;
pub mod manifest;
pub mod organizer;
pub mod scanner;
pub mod storage;
pub mod types;
pub mod update;

pub use burst::*;
pub use catalog::{generate_html_catalog, CatalogItem, DEFAULT_CATALOG_FILENAME};
pub use classifier::*;
pub use cli::*;
pub use dataset::*;
pub use db::*;
pub use detector::*;
pub use geo::*;
pub use help::*;
pub use location::*;
pub use organizer::*;
pub use scanner::*;
pub use storage::*;
pub use types::*;
pub use update::*;
