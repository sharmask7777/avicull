//! # Core Domain Models and Shared Types
//!
//! This module defines the foundational data contracts used throughout `avicull`:
//! - [`AssetBundle`]: The atomic representation of an image asset along with companion files (e.g. RAW + JPEG) and sidecars (`.xmp`).
//! - [`AssetCategory`]: High-level taxonomic domain or photographic category (e.g. Bird, Mammalia, Insecta, Scenery).
//! - [`SpeciesPrediction`]: Multi-class avian predictions with visual probabilities and Bayesian geographic priors.
//! - [`BundleDecision`]: The finalized classification, routing tier, and consensus confidence for an asset.
//! - [`RoutingTier`]: The tri-state sorting tier determining directory routing (`ConfirmedSpecies`, `NeedsReview`, `NoBirdDetected`).
//! - [`OrganizationMode`]: The file organization strategy (`InPlace`, `InPlaceRename`, `HierarchicalHardlinkHierarchy`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

pub const CURATED_MULTI_TAXA_JSON: &str = include_str!("../data/curated_multi_taxa.json");
static TAXONOMY_JSON: &str = include_str!("../data/taxonomy.json");

static KNOWN_BIRD_SET: LazyLock<std::collections::HashSet<String>> = LazyLock::new(|| {
    let mut set = std::collections::HashSet::new();

    // Ingest authoritative eBird taxonomy (11,240+ species) from canonical dataset
    if let Ok(records) = serde_json::from_str::<Vec<serde_json::Value>>(TAXONOMY_JSON) {
        for r in records {
            let cat = r.get("category").and_then(|v| v.as_str()).unwrap_or("Bird");
            if cat == "Bird" {
                if let Some(tid) = r.get("taxon_id").and_then(|v| v.as_str()) {
                    set.insert(tid.to_lowercase());
                }
                if let Some(cname) = r.get("common_name").and_then(|v| v.as_str()) {
                    let lower = cname.to_lowercase();
                    set.insert(lower.clone());
                    let slug = lower.replace(' ', "_");
                    set.insert(slug.clone());
                    set.insert(slug.replace('-', "_"));
                    set.insert(lower.replace('-', " "));
                }
                if let Some(slug) = r.get("folder_slug").and_then(|v| v.as_str()) {
                    let lower = slug.to_lowercase();
                    set.insert(lower.clone());
                    set.insert(lower.replace('-', "_"));
                    set.insert(lower.replace('_', " "));
                }
                if let Some(sci) = r.get("scientific_name").and_then(|v| v.as_str()) {
                    let lower = sci.to_lowercase();
                    set.insert(lower.clone());
                    set.insert(lower.replace(' ', "_"));
                }
            }
        }
    }

    set
});

static CANONICAL_BIRD_COMMON_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut names = Vec::new();
    if let Ok(records) = serde_json::from_str::<Vec<serde_json::Value>>(TAXONOMY_JSON) {
        for r in records {
            let cat = r.get("category").and_then(|v| v.as_str()).unwrap_or("Bird");
            if cat == "Bird" {
                if let Some(cname) = r.get("common_name").and_then(|v| v.as_str()) {
                    names.push(cname.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
});

/// Returns the slice of all canonical eBird bird common names.
#[must_use]
pub fn canonical_bird_common_names() -> &'static [String] {
    &CANONICAL_BIRD_COMMON_NAMES
}

/// Checks whether an entity slug or species name belongs to authoritative eBird taxonomy.
#[must_use]
pub fn is_known_bird_slug(slug: &str) -> bool {
    let lower = slug.to_lowercase();
    let clean = lower.trim_start_matches('_');
    if clean.is_empty() {
        return false;
    }
    if KNOWN_BIRD_SET.contains(clean) {
        return true;
    }
    let hyph = clean.replace('-', "_");
    if KNOWN_BIRD_SET.contains(&hyph) {
        return true;
    }
    let spaces = clean.replace('_', " ");
    if KNOWN_BIRD_SET.contains(&spaces) {
        return true;
    }
    let spaces_hyph = clean.replace('-', " ");
    KNOWN_BIRD_SET.contains(&spaces_hyph)
}

static CURATED_MULTI_TAXA_MAP: LazyLock<HashMap<String, AssetCategory>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    if let Ok(records) = serde_json::from_str::<Vec<serde_json::Value>>(CURATED_MULTI_TAXA_JSON) {
        for r in records {
            if let Some(cat_str) = r.get("category").and_then(|v| v.as_str()) {
                let cat = match cat_str {
                    "Mammalia" => AssetCategory::Mammalia,
                    "Insecta" => AssetCategory::Insecta,
                    "Reptilia" => AssetCategory::Reptilia,
                    "Amphibia" => AssetCategory::Amphibia,
                    "Scenery" => AssetCategory::Scenery,
                    "Architecture" => AssetCategory::Architecture,
                    "Portrait" => AssetCategory::Portrait,
                    "Abstract" => AssetCategory::Abstract,
                    _ => AssetCategory::NoSubjectDetected,
                };
                if let Some(tid) = r.get("taxon_id").and_then(|v| v.as_str()) {
                    let lower = tid.to_lowercase();
                    map.insert(lower.clone(), cat.clone());
                    map.insert(lower.replace('-', "_"), cat.clone());
                }
                if let Some(cname) = r.get("common_name").and_then(|v| v.as_str()) {
                    let lower = cname.to_lowercase();
                    map.insert(lower.clone(), cat.clone());
                    map.insert(lower.replace('-', "_"), cat.clone());
                    map.insert(lower.replace('_', " "), cat.clone());
                    let slug = lower.replace(' ', "_");
                    map.insert(slug.clone(), cat.clone());
                    map.insert(slug.replace('-', "_"), cat.clone());
                }
                if let Some(slug) = r.get("folder_slug").and_then(|v| v.as_str()) {
                    let lower = slug.to_lowercase();
                    map.insert(lower.clone(), cat.clone());
                    map.insert(lower.replace('-', "_"), cat.clone());
                    map.insert(lower.replace('_', " "), cat.clone());
                }
                if let Some(aliases) = r.get("aliases").and_then(|v| v.as_array()) {
                    for a in aliases {
                        if let Some(s) = a.as_str() {
                            let lower = s.to_lowercase();
                            map.insert(lower.clone(), cat.clone());
                            map.insert(lower.replace('-', "_"), cat.clone());
                            map.insert(lower.replace('_', " "), cat.clone());
                            let slug = lower.replace(' ', "_");
                            map.insert(slug.clone(), cat.clone());
                            map.insert(slug.replace('-', "_"), cat.clone());
                        }
                    }
                }
            }
        }
    }
    map
});

/// Returns a reference to the shared curated multi-taxa category map.
#[must_use]
pub fn curated_multi_taxa_map() -> &'static HashMap<String, AssetCategory> {
    &CURATED_MULTI_TAXA_MAP
}

/// Returns the canonical embedded eBird and multi-taxa taxonomy JSON content.
#[must_use]
pub fn raw_taxonomy_json() -> &'static str {
    TAXONOMY_JSON
}

/// High-level taxonomic domain or photographic genre category.
///
/// Categories are strictly singular:
/// - Avian fauna: [`AssetCategory::Bird`]
/// - Mammals: [`AssetCategory::Mammalia`]
/// - Insects: [`AssetCategory::Insecta`]
/// - Reptiles: [`AssetCategory::Reptilia`]
/// - Amphibians: [`AssetCategory::Amphibia`]
/// - Scenery and landscapes: [`AssetCategory::Scenery`]
/// - Architecture (buildings, monuments): [`AssetCategory::Architecture`]
/// - Portrait (people, portraits): [`AssetCategory::Portrait`]
/// - Abstract (patterns, textures): [`AssetCategory::Abstract`]
/// - Unclassified or empty frames: [`AssetCategory::NoSubjectDetected`]
/// - Extensible custom genres: [`AssetCategory::Other`]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AssetCategory {
    /// Unclassified or zero-detection empty frames (`_No_Bird_Detected` / `_No_Subject_Detected`).
    #[default]
    NoSubjectDetected,
    /// Avian species.
    Bird,
    /// Class Mammalia (mammals).
    Mammalia,
    /// Class Insecta (insects, butterflies, dragonflies).
    Insecta,
    /// Class Reptilia (reptiles: snakes, lizards, turtles, crocodilians).
    Reptilia,
    /// Class Amphibia (amphibians: frogs, toads, salamanders).
    Amphibia,
    /// Scenery, landscape, and environmental motifs.
    Scenery,
    /// Built architecture, monuments, buildings, and structures.
    Architecture,
    /// Portraits and human figures.
    Portrait,
    /// Abstract, textures, patterns, and minimalist geometry.
    Abstract,
    /// Extensible photographic genres.
    Other(String),
}

impl AssetCategory {
    /// Backward-compatible alias for [`AssetCategory::NoSubjectDetected`].
    #[allow(non_upper_case_globals)]
    pub const NoBirdDetected: Self = Self::NoSubjectDetected;

    /// Returns the canonical singular string representation of this category.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Bird => "Bird",
            Self::Mammalia => "Mammalia",
            Self::Insecta => "Insecta",
            Self::Reptilia => "Reptilia",
            Self::Amphibia => "Amphibia",
            Self::Scenery => "Scenery",
            Self::Architecture => "Architecture",
            Self::Portrait => "Portrait",
            Self::Abstract => "Abstract",
            Self::NoSubjectDetected => "No_Subject_Detected",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Resolves an asset category from an assigned species or entity slug.
    ///
    /// # Arguments
    /// * `slug` - Assigned taxon or motif directory slug.
    ///
    /// # Returns
    /// The resolved [`AssetCategory`], returning [`AssetCategory::NoSubjectDetected`]
    /// for unknown or unclassified entities. Under no circumstance does this method
    /// assume that unknown slugs are birds.
    #[must_use]
    pub fn from_slug(slug: &str) -> Self {
        let lower = slug.to_lowercase();
        let clean = lower.trim_start_matches('_');
        if clean.starts_with("no_bird_detected")
            || clean.starts_with("no_subject_detected")
            || clean.starts_with("nobirddetected")
            || clean.starts_with("nosubjectdetected")
            || clean.is_empty()
            || clean == "unknown"
        {
            return Self::NoSubjectDetected;
        }

        // 1. Explicit Category Prefixes (e.g. "Bird/...", "Bird_...", "Mammalia_...")
        if clean.starts_with("bird/") || clean.starts_with("bird_") {
            return Self::Bird;
        } else if clean.starts_with("mammalia") || clean.starts_with("mammal") {
            return Self::Mammalia;
        } else if clean.starts_with("insecta") || clean.starts_with("insect") {
            return Self::Insecta;
        } else if clean.starts_with("reptilia") || clean.starts_with("reptile") {
            return Self::Reptilia;
        } else if clean.starts_with("amphibia") || clean.starts_with("amphibian") {
            return Self::Amphibia;
        } else if clean.starts_with("scenery") || clean.starts_with("landscape") {
            return Self::Scenery;
        } else if clean.starts_with("architecture") || clean.starts_with("building") {
            return Self::Architecture;
        } else if clean.starts_with("portrait") {
            return Self::Portrait;
        } else if clean.starts_with("abstract") {
            return Self::Abstract;
        }

        // 2. Direct match in curated non-avian registry (exact taxon_id, common_name, or folder_slug)
        if let Some(cat) = CURATED_MULTI_TAXA_MAP.get(clean) {
            return cat.clone();
        }
        let clean_hyph = clean.replace('-', "_");
        if let Some(cat) = CURATED_MULTI_TAXA_MAP.get(&clean_hyph) {
            return cat.clone();
        }
        let with_spaces = clean.replace('_', " ");
        if let Some(cat) = CURATED_MULTI_TAXA_MAP.get(&with_spaces) {
            return cat.clone();
        }

        // 3. Exact match against folder slug suffix (e.g. "portrait/cultural_street_portrait" -> "cultural_street_portrait")
        if let Some(pos) = clean.rfind('/') {
            let suffix = &clean[pos + 1..];
            if let Some(cat) = CURATED_MULTI_TAXA_MAP.get(suffix) {
                return cat.clone();
            }
        }

        // 4. Known bird taxonomy validation
        if is_known_bird_slug(clean)
            || is_known_bird_slug(&clean_hyph)
            || is_known_bird_slug(&with_spaces)
        {
            return Self::Bird;
        }

        // 5. Unrecognized, ambiguous, or unclassified slug:
        // Invariant: NEVER assume unknown entities are birds!
        Self::NoSubjectDetected
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
        } else if trimmed.eq_ignore_ascii_case("reptilia")
            || trimmed.eq_ignore_ascii_case("reptile")
            || trimmed.eq_ignore_ascii_case("reptiles")
        {
            Ok(Self::Reptilia)
        } else if trimmed.eq_ignore_ascii_case("amphibia")
            || trimmed.eq_ignore_ascii_case("amphibian")
            || trimmed.eq_ignore_ascii_case("amphibians")
        {
            Ok(Self::Amphibia)
        } else if trimmed.eq_ignore_ascii_case("scenery")
            || trimmed.eq_ignore_ascii_case("landscape")
        {
            Ok(Self::Scenery)
        } else if trimmed.eq_ignore_ascii_case("architecture")
            || trimmed.eq_ignore_ascii_case("building")
            || trimmed.eq_ignore_ascii_case("buildings")
        {
            Ok(Self::Architecture)
        } else if trimmed.eq_ignore_ascii_case("portrait")
            || trimmed.eq_ignore_ascii_case("portraits")
        {
            Ok(Self::Portrait)
        } else if trimmed.eq_ignore_ascii_case("abstract") {
            Ok(Self::Abstract)
        } else if trimmed.eq_ignore_ascii_case("no_subject_detected")
            || trimmed.eq_ignore_ascii_case("_no_subject_detected")
            || trimmed.eq_ignore_ascii_case("no subject detected")
            || trimmed.eq_ignore_ascii_case("no_bird_detected")
            || trimmed.eq_ignore_ascii_case("_no_bird_detected")
            || trimmed.eq_ignore_ascii_case("no bird detected")
        {
            Ok(Self::NoSubjectDetected)
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

impl SpeciesPrediction {
    /// Resolves the taxonomic or photographic category of this prediction.
    #[must_use]
    pub fn category(&self) -> AssetCategory {
        let from_slug = AssetCategory::from_slug(&self.folder_slug);
        if from_slug != AssetCategory::Bird {
            return from_slug;
        }
        let from_tid = AssetCategory::from_slug(&self.taxon_id);
        if from_tid != AssetCategory::Bird {
            return from_tid;
        }
        AssetCategory::from_slug(&self.common_name)
    }
}

/// Domain-extensible alias for [`SpeciesPrediction`].
pub type TaxonPrediction = SpeciesPrediction;

/// Strongly-typed classification target encompassing broad category and optional granular entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    /// Domain category: Bird, Mammalia, Reptilia, Amphibia, Insecta,
    /// Scenery, Portrait, Architecture, Abstract, NoSubjectDetected
    pub category: AssetCategory,

    /// Granular entity or species slug, or `None` for top-level kingdom or genre filing.
    /// e.g. `Some("Snow_Leopard")`, `Some("Mountain")`, or `None` for top-level "Mammalia" / "Scenery".
    pub entity_slug: Option<String>,
}

impl Classification {
    /// Constructs a top-level category classification without a granular species/entity.
    #[must_use]
    pub fn top_level(category: AssetCategory) -> Self {
        Self {
            category,
            entity_slug: None,
        }
    }

    /// Constructs a granular species or entity classification.
    #[must_use]
    pub fn granular(category: AssetCategory, entity_slug: impl Into<String>) -> Self {
        let slug = entity_slug.into();
        let entity_slug = if slug.trim().is_empty() {
            None
        } else {
            Some(slug)
        };
        Self {
            category,
            entity_slug,
        }
    }

    /// Resolves classification from a species or entity slug.
    #[must_use]
    pub fn from_slug(slug: &str) -> Self {
        let cat = AssetCategory::from_slug(slug);
        let trimmed = slug.trim();
        let entity_slug = if trimmed.is_empty()
            || trimmed == "_No_Bird_Detected"
            || trimmed == "_No_Subject_Detected"
            || trimmed == "No_Subject_Detected"
            || trimmed == "No_Bird_Detected"
        {
            None
        } else {
            Some(trimmed.to_string())
        };
        Self {
            category: cat,
            entity_slug,
        }
    }
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
    /// Strongly-typed classification target.
    pub classification: Classification,
    /// Destination sorting tier based on detection and classification thresholds.
    pub routing_tier: RoutingTier,
    /// Whether this decision was forced by explicit human curation or manual override.
    #[serde(default)]
    pub is_user_override: bool,
}

impl BundleDecision {
    /// Returns the primary domain category.
    #[must_use]
    pub fn category(&self) -> &AssetCategory {
        &self.classification.category
    }

    /// Returns the optional granular species or entity slug.
    #[must_use]
    pub fn entity_slug(&self) -> Option<&str> {
        self.classification.entity_slug.as_deref()
    }

    /// Backwards-compatible slug accessor for logging and legacy formatters.
    #[must_use]
    pub fn assigned_species_slug(&self) -> &str {
        if let Some(slug) = self.classification.entity_slug.as_deref() {
            slug
        } else if self.routing_tier == RoutingTier::NoBirdDetected {
            RoutingTier::SENTINEL_NO_BIRD_DETECTED
        } else {
            ""
        }
    }

    /// Returns destination path tokens: `(category, optional_subfolder)`.
    #[must_use]
    pub fn destination_path_tokens(&self) -> (&str, Option<&str>) {
        (self.category().as_str(), self.entity_slug())
    }
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

impl RoutingTier {
    /// Domain-extensible alias for [`RoutingTier::NoBirdDetected`].
    #[allow(non_upper_case_globals)]
    pub const NoSubjectDetected: Self = Self::NoBirdDetected;

    /// Directory name for assets requiring human review.
    pub const DIR_NEEDS_REVIEW: &'static str = "_Needs_Review";

    /// Path prefix for items in the Needs Review hierarchy (`_Needs_Review/`).
    pub const PREFIX_NEEDS_REVIEW: &'static str = "_Needs_Review/";

    /// Directory name for assets where no focal subject was detected.
    pub const DIR_NO_SUBJECT_DETECTED: &'static str = "_No_Subject_Detected";

    /// Legacy directory name for backward compatibility with older archives.
    pub const DIR_NO_BIRD_DETECTED: &'static str = "_No_Bird_Detected";

    /// Directory name for corrupted or unreadable images.
    pub const DIR_CORRUPTED: &'static str = "_Corrupted_Images";

    /// Default output hierarchy root directory name.
    pub const DEFAULT_OUTPUT_DIR: &'static str = "birds_hierarchy";

    /// Sentinel species slug for assets where no bird or focal subject was detected.
    pub const SENTINEL_NO_BIRD_DETECTED: &'static str = "_No_Bird_Detected";

    /// Sentinel species slug for generic non-avian or unclassified subjects.
    pub const SENTINEL_NO_SUBJECT_DETECTED: &'static str = "_No_Subject_Detected";

    /// Descriptive in-place filename token for items needing human review.
    pub const TOKEN_NEEDS_REVIEW: &'static str = "Needs_Review";

    /// Descriptive in-place filename token for unclassified assets without a focal subject.
    pub const TOKEN_NO_SUBJECT_DETECTED: &'static str = "No_Subject_Detected";

    /// Descriptive in-place filename token for legacy avian zero-detection frames.
    pub const TOKEN_NO_BIRD_DETECTED: &'static str = "No_Bird_Detected";

    /// Returns the canonical directory name within the organized hierarchy for this routing tier,
    /// or `None` if the asset is routed directly under its confirmed species/category directory.
    #[must_use]
    pub const fn dir_name(&self) -> Option<&'static str> {
        match self {
            Self::ConfirmedSpecies => None,
            Self::NeedsReview => Some(Self::DIR_NEEDS_REVIEW),
            Self::NoBirdDetected => Some(Self::DIR_NO_SUBJECT_DETECTED),
        }
    }
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
    /// Non-destructive in-place cataloging mode (default): keeps source files strictly untouched,
    /// writing all classification metadata, previews, and HTML catalog into `.avicull/` and `catalog.html`.
    #[default]
    #[value(name = "in-place", alias = "no-rename")]
    InPlace,

    /// Atomically renames source files in-place with descriptive species, date, and location tokens.
    #[value(
        name = "in-place-rename",
        alias = "in-place-file-rename-to-descriptive-names"
    )]
    InPlaceRename,

    /// Materializes a zero-byte taxonomy hierarchy (`birds_hierarchy/`) using APFS hard links.
    #[value(
        name = "hierarchy",
        alias = "hierarchical-hardlink-hierarchy",
        alias = "hardlink"
    )]
    HierarchicalHardlinkHierarchy,
}

impl OrganizationMode {
    /// Backwards-compatible alias for [`OrganizationMode::InPlaceRename`].
    #[allow(non_upper_case_globals)]
    pub const InPlaceFileRenameToDescriptiveNames: Self = Self::InPlaceRename;
}

impl std::fmt::Display for OrganizationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizationMode::InPlace => write!(f, "in-place"),
            OrganizationMode::InPlaceRename => write!(f, "in-place-rename"),
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
