pub mod template;

use crate::scanner::extract_embedded_jpeg_preview;
use crate::types::{AssetBundle, BundleDecision, OrganizationMode, RoutingTier, SpeciesPrediction};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Standard default filename for the HTML bird photo catalog.
pub const DEFAULT_CATALOG_FILENAME: &str = "bird_catalog.html";

/// Individual photo bundle item represented in the HTML catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub bundle_id: String,
    pub original_stem: String,
    pub current_path: PathBuf,
    pub primary_image: PathBuf,
    pub companion_images: Vec<PathBuf>,
    pub sidecar_files: Vec<PathBuf>,
    pub relative_preview_path: Option<String>,
    pub assigned_species: String,
    pub scientific_name: Option<String>,
    pub routing_tier: RoutingTier,
    pub confidence: f32,
    pub capture_date: Option<String>,
    pub location_name: Option<String>,
    pub top_predictions: Vec<SpeciesPrediction>,
}

/// Backward and forward compatible alias so calling code can reference ClassifiedBundleItem.
pub type ClassifiedBundleItem = CatalogItem;

impl CatalogItem {
    /// Creates a CatalogItem by combining an AssetBundle and its final BundleDecision.
    pub fn from_bundle_and_decision(
        bundle: &AssetBundle,
        decision: &BundleDecision,
        current_primary_path: Option<PathBuf>,
        companion_paths: Option<Vec<PathBuf>>,
        sidecar_paths: Option<Vec<PathBuf>>,
    ) -> Self {
        let (assigned_species, scientific_name) = match decision.routing_tier {
            RoutingTier::NoBirdDetected => ("_No_Bird_Detected".to_string(), None),
            _ => {
                if let Some(top) = decision.top_predictions.first() {
                    (top.common_name.clone(), Some(top.scientific_name.clone()))
                } else {
                    let cleaned = decision.assigned_species_slug.replace('_', " ");
                    (cleaned, None)
                }
            }
        };

        let capture_date = bundle.capture_timestamp_ms.and_then(|ts| {
            chrono::DateTime::from_timestamp_millis(ts).map(|dt| dt.format("%Y-%m-%d").to_string())
        });

        Self {
            bundle_id: bundle.bundle_id.clone(),
            original_stem: bundle.original_stem.clone(),
            current_path: current_primary_path.unwrap_or_else(|| bundle.primary_image.clone()),
            primary_image: bundle.primary_image.clone(),
            companion_images: companion_paths.unwrap_or_else(|| bundle.companion_images.clone()),
            sidecar_files: sidecar_paths.unwrap_or_else(|| bundle.sidecar_files.clone()),
            relative_preview_path: None,
            assigned_species,
            scientific_name,
            routing_tier: decision.routing_tier,
            confidence: decision.final_confidence,
            capture_date,
            location_name: bundle.resolved_location_name.clone(),
            top_predictions: decision.top_predictions.clone(),
        }
    }
}

/// Statistics payload embedded into the client-side catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogStats {
    pub total_photos: usize,
    pub species_count: usize,
    pub confirmed_count: usize,
    pub needs_review_count: usize,
    pub no_bird_count: usize,
}

/// Full JSON data structure embedded inside <script id="catalog-data">.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPayload {
    pub trip_name: String,
    pub mode: String,
    pub catalog_dir: String,
    pub generated_at: String,
    pub stats: CatalogStats,
    pub items: Vec<CatalogPayloadItem>,
    pub species_list: Vec<String>,
}

/// JSON representation of an individual item in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPayloadItem {
    pub id: String,
    pub original_stem: String,
    pub current_path: String,
    pub primary_filename: String,
    pub preview_url: String,
    pub assigned_species: String,
    pub scientific_name: Option<String>,
    pub routing_tier: String,
    pub confidence: f32,
    pub capture_date: Option<String>,
    pub location_name: Option<String>,
    pub companion_files: Vec<String>,
    pub sidecar_files: Vec<String>,
    pub top_predictions: Vec<PayloadPrediction>,
}

/// Serializable species prediction candidate for the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadPrediction {
    pub common_name: String,
    pub scientific_name: String,
    pub score: f32,
}

/// Generates a standalone, zero-dependency offline HTML photo catalog.
pub fn generate_html_catalog(
    trip_name: &str,
    items: &[CatalogItem],
    mode: OrganizationMode,
    output_path: &Path,
) -> Result<()> {
    let output_root = output_path.parent().unwrap_or(Path::new("."));
    if !output_root.as_os_str().is_empty() && !output_root.exists() {
        fs::create_dir_all(output_root)
            .with_context(|| format!("Failed to create catalog directory: {:?}", output_root))?;
    }

    let mut payload_items = Vec::with_capacity(items.len());
    let mut confirmed_count = 0;
    let mut needs_review_count = 0;
    let mut no_bird_count = 0;
    let mut unique_species = HashSet::new();

    for item in items {
        match item.routing_tier {
            RoutingTier::ConfirmedSpecies => {
                confirmed_count += 1;
                if !item.assigned_species.starts_with('_') && !item.assigned_species.is_empty() {
                    unique_species.insert(item.assigned_species.clone());
                }
            }
            RoutingTier::NeedsReview => {
                needs_review_count += 1;
                if !item.assigned_species.starts_with('_') && !item.assigned_species.is_empty() {
                    unique_species.insert(item.assigned_species.clone());
                }
            }
            RoutingTier::NoBirdDetected => {
                no_bird_count += 1;
            }
        }

        let preview_url = resolve_or_extract_preview(item, output_root);

        let current_path_rel = to_relative_string(&item.current_path, output_root);
        let primary_filename = item
            .primary_image
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.jpg")
            .to_string();

        let companion_files: Vec<String> = item
            .companion_images
            .iter()
            .map(|p| to_relative_string(p, output_root))
            .collect();

        let sidecar_files: Vec<String> = item
            .sidecar_files
            .iter()
            .map(|p| to_relative_string(p, output_root))
            .collect();

        let top_preds: Vec<PayloadPrediction> = item
            .top_predictions
            .iter()
            .map(|p| PayloadPrediction {
                common_name: p.common_name.clone(),
                scientific_name: p.scientific_name.clone(),
                score: p.combined_score,
            })
            .collect();

        let tier_str = match item.routing_tier {
            RoutingTier::ConfirmedSpecies => "ConfirmedSpecies",
            RoutingTier::NeedsReview => "NeedsReview",
            RoutingTier::NoBirdDetected => "NoBirdDetected",
        };

        payload_items.push(CatalogPayloadItem {
            id: item.bundle_id.clone(),
            original_stem: item.original_stem.clone(),
            current_path: current_path_rel,
            primary_filename,
            preview_url,
            assigned_species: item.assigned_species.clone(),
            scientific_name: item.scientific_name.clone(),
            routing_tier: tier_str.to_string(),
            confidence: item.confidence,
            capture_date: item.capture_date.clone(),
            location_name: item.location_name.clone(),
            companion_files,
            sidecar_files,
            top_predictions: top_preds,
        });
    }

    let stats = CatalogStats {
        total_photos: items.len(),
        species_count: unique_species.len(),
        confirmed_count,
        needs_review_count,
        no_bird_count,
    };

    let mut species_list = load_species_taxonomy_list();
    for item in items {
        if !item.assigned_species.starts_with('_') && !item.assigned_species.is_empty() {
            species_list.push(item.assigned_species.clone());
        }
        for pred in &item.top_predictions {
            if !pred.common_name.is_empty() {
                species_list.push(pred.common_name.clone());
            }
        }
    }
    species_list.sort();
    species_list.dedup();

    let mode_str = match mode {
        OrganizationMode::HierarchicalHardlinkHierarchy => "hierarchy",
        OrganizationMode::InPlaceFileRenameToDescriptiveNames => "in-place",
    };

    let catalog_dir_str = output_root
        .canonicalize()
        .unwrap_or_else(|_| output_root.to_path_buf())
        .to_string_lossy()
        .to_string();

    let payload = CatalogPayload {
        trip_name: trip_name.to_string(),
        mode: mode_str.to_string(),
        catalog_dir: catalog_dir_str,
        generated_at: Utc::now().to_rfc3339(),
        stats,
        items: payload_items,
        species_list,
    };

    let html_content = template::render_catalog_html(&payload);
    fs::write(output_path, html_content)
        .with_context(|| format!("Failed to write catalog HTML to {:?}", output_path))?;

    Ok(())
}

/// Resolves or extracts preview thumbnail path for an item relative to output_root.
fn resolve_or_extract_preview(item: &CatalogItem, output_root: &Path) -> String {
    // 1. Explicit preview already provided
    if let Some(ref p) = item.relative_preview_path {
        if !p.is_empty() {
            return p.clone();
        }
    }

    // 2. Companion JPEG file
    if let Some(comp) = item.companion_images.first() {
        if comp.exists() {
            return to_relative_string(comp, output_root);
        }
    }

    let active_primary = if item.current_path.exists() {
        &item.current_path
    } else {
        &item.primary_image
    };

    // 3. Web-renderable primary image
    let is_web_img = active_primary
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp")
        })
        .unwrap_or(false);

    if is_web_img && active_primary.exists() {
        return to_relative_string(active_primary, output_root);
    }

    // 4. Standalone camera RAW file: extract embedded JPEG preview thumbnail
    if active_primary.exists() {
        if let Ok(raw_bytes) = fs::read(active_primary) {
            if let Some(preview_bytes) = extract_embedded_jpeg_preview(&raw_bytes) {
                let previews_dir = output_root.join(".avicull").join("previews");
                if fs::create_dir_all(&previews_dir).is_ok() {
                    let preview_name = format!("{}.jpg", item.original_stem);
                    let target_file = previews_dir.join(&preview_name);
                    if fs::write(&target_file, &preview_bytes).is_ok() {
                        return format!(".avicull/previews/{}", preview_name);
                    }
                }
            }
        }
    }

    // 5. Fallback relative path (or string representation) for onerror browser fallback
    to_relative_string(active_primary, output_root)
}

/// Formats a path relative to root when possible.
fn to_relative_string(path: &Path, root: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        rel.to_string_lossy().to_string()
    } else {
        path.to_string_lossy().to_string()
    }
}

/// Loads full eBird species taxonomy if cached or local models exist,
/// falling back to a curated global list.
fn load_species_taxonomy_list() -> Vec<String> {
    let mut candidate_paths = vec![
        PathBuf::from("models/taxonomy.json"),
        PathBuf::from("../models/taxonomy.json"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        candidate_paths.push(
            PathBuf::from(home)
                .join(".cache")
                .join("avicull")
                .join("models")
                .join("taxonomy.json"),
        );
    }

    for path in candidate_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    let mut list = Vec::with_capacity(parsed.len());
                    for obj in parsed {
                        if let Some(name) = obj.get("common_name").and_then(|v| v.as_str()) {
                            list.push(name.to_string());
                        }
                    }
                    if !list.is_empty() {
                        return list;
                    }
                }
            }
        }
    }

    fallback_curated_species()
}

/// Fallback list of prominent bird species across world families.
fn fallback_curated_species() -> Vec<String> {
    vec![
        "House Sparrow",
        "Eurasian Tree Sparrow",
        "Black Redstart",
        "Common Redstart",
        "Tibetan Snowcock",
        "Himalayan Snowcock",
        "Lammergeier",
        "Golden Eagle",
        "Steppe Eagle",
        "Painted Stork",
        "Asian Openbill",
        "Black-necked Crane",
        "Common Crane",
        "Bar-headed Goose",
        "Ruddy Shelduck",
        "Mallard",
        "Common Kingfisher",
        "White-throated Kingfisher",
        "Pied Kingfisher",
        "Little Egret",
        "Great Egret",
        "Grey Heron",
        "Peregrine Falcon",
        "Common Kestrel",
        "Barn Owl",
        "Spotted Owlet",
        "Eurasian Eagle-Owl",
        "Blue Rock Thrush",
        "Great Tit",
        "Coal Tit",
        "Eurasian Magpie",
        "Red-billed Chough",
        "Yellow-billed Chough",
        "Brown-headed Gull",
        "Common Sandpiper",
        "Red-wattled Lapwing",
        "Little Ringed Plover",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
