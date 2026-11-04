//! # Core Domain Models and Shared Types
//!
//! This module defines the foundational data contracts used throughout `avicull`:
//! - [`AssetBundle`]: The atomic representation of an image asset along with companion files (e.g. RAW + JPEG) and sidecars (`.xmp`).
//! - [`AssetCategory`]: High-level taxonomic domain or photographic category (e.g. Bird, Mammalia, Insecta, Scenery).
//! - [`SpeciesPrediction`]: Multi-class avian predictions with visual probabilities and Bayesian geographic priors.
//! - [`BundleDecision`]: The finalized classification, routing tier, and consensus confidence for an asset.
//! - [`RoutingTier`]: The tri-state sorting tier determining directory routing (`ConfirmedSpecies`, `NeedsReview`, `NoBirdDetected`).
//! - [`OrganizationMode`]: The file organization strategy (`HierarchicalHardlinkHierarchy` vs `InPlaceFileRenameToDescriptiveNames`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// High-level taxonomic domain or photographic genre category.
///
/// Categories are strictly singular:
/// - Avian fauna: [`AssetCategory::Bird`]
/// - Mammals: [`AssetCategory::Mammalia`]
/// - Insects: [`AssetCategory::Insecta`]
/// - Scenery and landscapes: [`AssetCategory::Scenery`]
/// - Extensible genres (e.g. Portrait, Abstract, Architecture): [`AssetCategory::Other`]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AssetCategory {
    /// Avian species (default taxonomic domain).
    #[default]
    Bird,
    /// Class Mammalia (mammals).
    Mammalia,
    /// Class Insecta (insects, butterflies, dragonflies).
    Insecta,
    /// Scenery, landscape, and environmental motifs.
    Scenery,
    /// Extensible photographic genres (e.g., `"Portrait"`, `"Abstract"`).
    Other(String),
}

impl AssetCategory {
    /// Returns the canonical singular string representation of this category.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Bird => "Bird",
            Self::Mammalia => "Mammalia",
            Self::Insecta => "Insecta",
            Self::Scenery => "Scenery",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Resolves an asset category from an assigned species or entity slug.
    ///
    /// # Arguments
    /// * `slug` - Assigned taxon or motif directory slug.
    ///
    /// # Returns
    /// The resolved [`AssetCategory`], defaulting to [`AssetCategory::Bird`].
    #[must_use]
    pub fn from_slug(slug: &str) -> Self {
        let lower = slug.to_lowercase();
        if lower.starts_with("mammalia") || lower.starts_with("mammal") {
            Self::Mammalia
        } else if lower.starts_with("insecta") || lower.starts_with("insect") {
            Self::Insecta
        } else if lower.starts_with("scenery") || lower.starts_with("landscape") {
            Self::Scenery
        } else if lower.starts_with("portrait") {
            Self::Other("Portrait".to_string())
        } else if lower.starts_with("abstract") {
            Self::Other("Abstract".to_string())
        } else {
            Self::Bird
        }
    }
}

impl std::fmt::Display for AssetCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for AssetCategory {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.eq_ignore_ascii_case("bird") || trimmed.eq_ignore_ascii_case("birds") {
            Ok(Self::Bird)
        } else if trimmed.eq_ignore_ascii_case("mammalia")
            || trimmed.eq_ignore_ascii_case("mammal")
            || trimmed.eq_ignore_ascii_case("mammals")
        {
            Ok(Self::Mammalia)
        } else if trimmed.eq_ignore_ascii_case("insecta")
            || trimmed.eq_ignore_ascii_case("insect")
            || trimmed.eq_ignore_ascii_case("insects")
        {
            Ok(Self::Insecta)
        } else if trimmed.eq_ignore_ascii_case("scenery") {
            Ok(Self::Scenery)
        } else {
            Ok(Self::Other(trimmed.to_string()))
        }
    }
}

/// An atomic multi-file photo bundle representing a single shutter capture.
///
/// Avian photography workflows frequently produce companion files alongside primary RAW files,
/// such as in-camera JPEGs (`.JPG`), manufacturer metadata (`.THM`), voice annotations (`.WAV`),
/// and editor metadata sidecars (`.xmp`, `.CR2.xmp`).
///
/// An `AssetBundle` encapsulates these related files to ensure they are moved, hard-linked,
/// renamed, or reclassified as an indivisible unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetBundle {
    /// BLAKE3 hash of the canonical absolute path of the primary image.
    pub bundle_id: String,
    /// APFS `st_ino` (inode number) of the primary image used for $O(1)$ duplicate detection.
    pub source_inode: u64,
    /// Immediate parent or top-level trip folder name (e.g., `"Leh"`, `"Bharatpur"`).
    pub trip_folder_name: String,
    /// Original file stem without extensions (e.g., `"DSC_4011"`).
    pub original_stem: String,
    /// The highest-priority image file (Priority order: RAW > JPEG > HEIC > PNG > TIFF).
    pub primary_image: PathBuf,
    /// Companion image files (e.g., `["DSC_4011.JPG"]` when primary is `DSC_4011.ARW`).
    pub companion_images: Vec<PathBuf>,
    /// Metadata sidecar files (e.g., `["DSC_4011.xmp", "DSC_4011.ARW.xmp"]`).
    pub sidecar_files: Vec<PathBuf>,
    /// Capture timestamp in UTC epoch milliseconds extracted from EXIF `DateTimeOriginal`.
    pub capture_timestamp_ms: Option<i64>,
    /// Geospatial coordinates from EXIF GPS tags or companion sidecars.
    pub gps_coords: Option<GpsCoordinate>,
    /// Human-readable location inferred from GPS, gazetteers, or trip folder names.
    #[serde(default)]
    pub resolved_location_name: Option<String>,
    /// Relative path of the bundle within the source archive root.
    #[serde(default)]
    pub relative_path: Option<PathBuf>,
}

/// Geospatial WGS84 coordinate pair with provenance source tracking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GpsCoordinate {
    /// WGS84 latitude in decimal degrees (-90.0 to +90.0).
    pub latitude: f64,
    /// WGS84 longitude in decimal degrees (-180.0 to +180.0).
    pub longitude: f64,
    /// Provenance origin of this coordinate.
    pub source: LocationSource,
}

/// Provenance hierarchy indicating how a geographic location was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationSource {
    /// Directly extracted from image EXIF GPS tags (`GPSLatitude`, `GPSLongitude`).
    ExifGps,
    /// Explicit user override defined per image stem or bundle ID.
    ImageLocationOverride,
    /// Inferred by matching an immediate subdirectory name against the gazetteer.
    SubdirectoryGazetteer,
    /// Inferred by matching the top-level trip root directory against the gazetteer.
    TripFolderGazetteer,
    /// Propagated from an adjacent image within the same temporal capture burst.
    SiblingPropagation,
    /// User-configured default fallback region.
    UserDefaultRegion,
}

/// A single candidate species prediction produced by neural inference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesPrediction {
    /// eBird / AviList 6-letter taxonomic code (e.g., `"paisto1"` for Painted Stork).
    pub taxon_id: String,
    /// Standardized English common name (e.g., `"Painted Stork"`).
    pub common_name: String,
    /// Filesystem-safe sanitized directory slug (e.g., `"Painted_Stork"`).
    pub folder_slug: String,
    /// Binomial scientific name (e.g., `"Mycteria leucocephala"`).
    pub scientific_name: String,
    /// Raw probability from the visual vision backbone classifier in `[0.0, 1.0]`.
    pub visual_prob: f32,
    /// Geographic prior probability conditioned on location and day of year in `[0.0, 1.0]`.
    pub geo_prior: f32,
    /// Bayesian posterior combination score in `[0.0, 1.0]`.
    pub combined_score: f32,
}

/// Final classification, routing tier, and confidence decision for an [`AssetBundle`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleDecision {
    /// Identifier matching [`AssetBundle::bundle_id`].
    pub bundle_id: String,
    /// Identifier of the temporal burst cluster this bundle belongs to, if clustered.
    pub burst_id: Option<String>,
    /// Visual detector confidence that an avian subject is present in the frame in `[0.0, 1.0]`.
    pub detection_confidence: f32,
    /// Normalized bounding box coordinates `[ymin, xmin, ymax, xmax]` in `[0.0, 1.0]`.
    pub bbox_xyxy_norm: Option<[f32; 4]>,
    /// Top species predictions sorted descending by combined posterior probability.
    pub top_predictions: Vec<SpeciesPrediction>,
    /// Top-1 confidence score of this single frame prior to burst consensus smoothing.
    pub frame_confidence: f32,
    /// Final confidence score (after burst consensus pooling) written to filenames.
    pub final_confidence: f32,
    /// Sanitized directory slug assigned for organization (e.g., `"Painted_Stork"`).
    pub assigned_species_slug: String,
    /// Destination sorting tier based on detection and classification thresholds.
    pub routing_tier: RoutingTier,
    /// Whether this decision was forced by explicit human curation or manual override.
    #[serde(default)]
    pub is_user_override: bool,
}

/// Sorting tier governing where an asset is organized within the destination hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingTier {
    /// Confidence exceeds `--min-confidence` threshold (default 0.65). Routed to species directory.
    ConfirmedSpecies,
    /// Bird detected, but confidence is below `--min-confidence`. Routed to `_Needs_Review/`.
    NeedsReview,
    /// Bird detector confidence is below `--min-detection-confidence` (default 0.25). Routed to `_No_Bird_Detected/`.
    NoBirdDetected,
}

/// Mode governing how confidence scores are calculated across photo bursts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceMode {
    /// Confidence-weighted pooling across all frames within a temporal burst ($\Delta t \le 3.0\text{s}$).
    BurstConsensus,
    /// Evaluates each frame independently without temporal smoothing.
    PerFrame,
}

/// Strategy for organizing the photo archive on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default, Serialize, Deserialize)]
pub enum OrganizationMode {
    /// Atomically renames source files in-place with descriptive species, date, and location tokens.
    #[default]
    #[value(name = "in-place", alias = "in-place-file-rename-to-descriptive-names")]
    InPlaceFileRenameToDescriptiveNames,

    /// Materializes a zero-byte taxonomy hierarchy (`birds_hierarchy/`) using APFS hard links.
    #[value(
        name = "hierarchy",
        alias = "hierarchical-hardlink-hierarchy",
        alias = "hardlink"
    )]
    HierarchicalHardlinkHierarchy,
}

impl std::fmt::Display for OrganizationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizationMode::InPlaceFileRenameToDescriptiveNames => write!(f, "in-place"),
            OrganizationMode::HierarchicalHardlinkHierarchy => write!(f, "hierarchy"),
        }
    }
}

/// Comprehensive error type representing failure modes across avicull operations.
#[derive(thiserror::Error, Debug)]
pub enum AvicullError {
    /// Source and destination reside on distinct filesystem volumes, preventing APFS hard links.
    #[error("Filesystem volume mismatch: input device {input_dev} != output device {output_dev}")]
    VolumeMismatch { input_dev: u64, output_dev: u64 },

    /// Destination directory is invalidly nested inside another monitored path.
    #[error("Disjointness violation: path '{nested}' is nested inside '{parent}'")]
    NestedPathDisjointness { nested: PathBuf, parent: PathBuf },

    /// macOS Transparency, Consent, and Control (TCC) prevented directory access.
    #[error(
        "Cannot read input directory: {}\n\n\
         macOS Privacy & Security (TCC) prevented reading this directory (Permission denied / Operation not permitted).\n\
         When accessing CloudStorage folders (Proton Drive, iCloud Drive, OneDrive, Dropbox, etc.), macOS requires Full Disk Access.\n\n\
         To resolve this:\n\
           1. Open macOS 'System Settings' -> 'Privacy & Security' -> 'Full Disk Access'.\n\
           2. Enable Full Disk Access for your Terminal, IDE (Antigravity / VS Code / Cursor), or shell application.\n\
           3. Re-run avicull.",
        path.display()
    )]
    CloudStorageAccessDenied { path: PathBuf },

    /// Operating system permission denied accessing the specified directory.
    #[error("Cannot read input directory: {} (Permission denied). Please check filesystem read permissions.", path.display())]
    DirectoryPermissionDenied { path: PathBuf },

    /// Hardlink hierarchy mode was requested on remote, cloud, or network-backed storage.
    #[error(
        "Hard-linked species hierarchy mode ('--mode hierarchy') is not supported on {storage_type}.\n\
         Creating hard-link hierarchies on cloud-synchronized or network drives triggers duplicate sync uploads\n\
         or cross-device link errors (EXDEV).\n\
         Please use the default '--mode in-place' for non-local storage."
    )]
    HierarchyModeUnsupportedOnRemote { storage_type: &'static str },

    /// Unrecognized confidence mode string passed on CLI.
    #[error("Unknown confidence mode: '{0}'. Expected 'burst-consensus' or 'per-frame'")]
    InvalidConfidenceMode(String),

    /// Database access or internal lock failure.
    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl std::str::FromStr for ConfidenceMode {
    type Err = AvicullError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "burst-consensus" | "burstconsensus" => Ok(ConfidenceMode::BurstConsensus),
            "per-frame" | "perframe" => Ok(ConfidenceMode::PerFrame),
            other => Err(AvicullError::InvalidConfidenceMode(other.to_string())),
        }
    }
}

impl std::fmt::Display for ConfidenceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfidenceMode::BurstConsensus => write!(f, "burst-consensus"),
            ConfidenceMode::PerFrame => write!(f, "per-frame"),
        }
    }
}
