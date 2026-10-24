use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceMode {
    BurstConsensus,
    PerFrame,
}

impl std::str::FromStr for ConfidenceMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "burst-consensus" | "burstconsensus" => Ok(ConfidenceMode::BurstConsensus),
            "per-frame" | "perframe" => Ok(ConfidenceMode::PerFrame),
            other => Err(format!("Unknown confidence mode: '{}'. Expected 'burst-consensus' or 'per-frame'", other)),
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
