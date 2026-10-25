use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetBundle {
    pub bundle_id: String,        // BLAKE3 hash of canonical primary_image path
    pub source_inode: u64,        // APFS st_ino of primary_image for instant deduplication
    pub trip_folder_name: String, // Immediate parent or root trip folder name
    pub original_stem: String,    // e.g., "DSC_4011"
    pub primary_image: PathBuf,   // Priority: RAW > JPEG > HEIC > PNG > TIFF
    pub companion_images: Vec<PathBuf>, // e.g., ["DSC_4011.JPG"] when primary is "DSC_4011.ARW"
    pub sidecar_files: Vec<PathBuf>, // e.g., ["DSC_4011.xmp", "DSC_4011.ARW.xmp"]
    pub capture_timestamp_ms: Option<i64>,
    pub gps_coords: Option<GpsCoordinate>,
    #[serde(default)]
    pub resolved_location_name: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesPrediction {
    pub taxon_id: String,        // eBird / AviList taxon code (e.g., "paisto1")
    pub common_name: String,     // e.g., "Painted Stork"
    pub folder_slug: String,     // Sanitized: "Painted_Stork"
    pub scientific_name: String, // e.g., "Mycteria leucocephala"
    pub visual_prob: f32,        // Raw visual model probability [0.0, 1.0]
    pub geo_prior: f32,          // Location/date occurrence prior [0.0, 1.0]
    pub combined_score: f32,     // Normalized posterior probability [0.0, 1.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleDecision {
    pub bundle_id: String,
    pub burst_id: Option<String>,
    pub detection_confidence: f32,
    pub bbox_xyxy_norm: Option<[f32; 4]>,
    pub top_predictions: Vec<SpeciesPrediction>, // Top-5 sorted descending
    pub frame_confidence: f32,                   // Top-1 score before burst smoothing
    pub final_confidence: f32,                   // Score written to hard-linked filename
    pub assigned_species_slug: String,
    pub routing_tier: RoutingTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingTier {
    ConfirmedSpecies, // final_confidence >= min_confidence (default 0.65)
    NeedsReview,      // bird detected, but final_confidence < min_confidence
    NoBirdDetected,   // detector confidence < min_detection_confidence (default 0.25)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceMode {
    BurstConsensus,
    PerFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default, Serialize, Deserialize)]
pub enum OrganizationMode {
    #[default]
    #[value(name = "in-place", alias = "in-place-file-rename-to-descriptive-names")]
    InPlaceFileRenameToDescriptiveNames,

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

#[derive(thiserror::Error, Debug)]
pub enum AvicullError {
    #[error("Filesystem volume mismatch: input device {input_dev} != output device {output_dev}")]
    VolumeMismatch { input_dev: u64, output_dev: u64 },
    #[error("Disjointness violation: path '{nested}' is nested inside '{parent}'")]
    NestedPathDisjointness { nested: PathBuf, parent: PathBuf },
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
    #[error("Cannot read input directory: {} (Permission denied). Please check filesystem read permissions.", path.display())]
    DirectoryPermissionDenied { path: PathBuf },
    #[error("Unknown confidence mode: '{0}'. Expected 'burst-consensus' or 'per-frame'")]
    InvalidConfidenceMode(String),
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
