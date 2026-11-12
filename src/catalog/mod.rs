//! # Standalone HTML5 Bird Photo Catalog Generation
//!
//! Generates a zero-dependency, self-contained single-file HTML5 review dashboard:
//! - **Client-Side Virtualized Grid**: Displays thousands of high-resolution avian photos smoothly with responsive cards.
//! - **Interactive Species Curation**: In-browser search, multi-selection (Cmd/Shift+Click), and bulk reclassification modals.
//! - **Automated Script Export**: Generates executable shell snippets to apply user corrections in seconds.
//! - **Embedded JSON Payload**: Stores state inside `<script id="catalog-data">` for Playwright testability and offline archiving.

pub mod template;

use crate::scanner::extract_embedded_jpeg_preview;
use crate::types::{
    AssetBundle, AssetCategory, BundleDecision, OrganizationMode, RoutingTier, SpeciesPrediction,
};
use anyhow::{Context, Result};
use chrono::Utc;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Curated non-avian taxon entry for catalog dynamic autocomplete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratedTaxonEntry {
    /// Unique identifier for this taxon or motif entry.
    pub taxon_id: String,
    /// Taxonomic or photographic genre category (e.g. `Mammalia`, `Insecta`, `Scenery`).
    pub category: String,
    /// Human-readable common name.
    pub common_name: String,
    /// Optional binomial scientific name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scientific_name: Option<String>,
    /// Search aliases and colloquial variants for client-side fuzzy autocomplete.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

static CURATED_MULTI_TAXA: LazyLock<Vec<CuratedTaxonEntry>> = LazyLock::new(|| {
    serde_json::from_str(crate::types::CURATED_MULTI_TAXA_JSON).unwrap_or_default()
});

/// Returns a clone of the curated non-avian multi-taxa entries for catalog autocomplete.
///
/// # Returns
/// A vector of [`CuratedTaxonEntry`] structs loaded from embedded static data.
#[must_use]
pub fn load_curated_multi_taxa() -> Vec<CuratedTaxonEntry> {
    CURATED_MULTI_TAXA.clone()
}

/// Standard default filename for the HTML bird photo catalog.
pub const DEFAULT_CATALOG_FILENAME: &str = "catalog.html";

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
    pub is_local: Option<bool>,
    pub assigned_species: String,
    pub scientific_name: Option<String>,
    pub routing_tier: RoutingTier,
    pub confidence: f32,
    pub capture_date: Option<String>,
    pub location_name: Option<String>,
    #[serde(default)]
    pub location_source: Option<String>,
    #[serde(default)]
    pub subdirectory: Option<String>,
    pub top_predictions: Vec<SpeciesPrediction>,
    #[serde(default)]
    pub category: AssetCategory,
}

impl CatalogItem {
    /// Returns a reference to the active on-disk image path, favoring `current_path`
    /// if it exists on disk (e.g. inside hierarchy output directory or post-rename),
    /// and falling back to `primary_image`.
    #[must_use]
    pub fn active_primary_path(&self) -> &Path {
        if self.current_path.exists() {
            &self.current_path
        } else {
            &self.primary_image
        }
    }
}

/// Backward and forward compatible alias so calling code can reference ClassifiedBundleItem.
pub type ClassifiedBundleItem = CatalogItem;

impl Default for CatalogItem {
    fn default() -> Self {
        Self {
            bundle_id: String::new(),
            original_stem: String::new(),
            current_path: PathBuf::new(),
            primary_image: PathBuf::new(),
            companion_images: Vec::new(),
            sidecar_files: Vec::new(),
            relative_preview_path: None,
            is_local: None,
            assigned_species: String::new(),
            scientific_name: None,
            routing_tier: RoutingTier::ConfirmedSpecies,
            confidence: 0.0,
            capture_date: None,
            location_name: None,
            location_source: None,
            subdirectory: None,
            top_predictions: Vec::new(),
            category: AssetCategory::NoSubjectDetected,
        }
    }
}

impl CatalogItem {
    /// Creates a CatalogItem by combining an AssetBundle and its final BundleDecision.
    pub fn from_bundle_and_decision(
        bundle: &AssetBundle,
        decision: &BundleDecision,
        current_primary_path: Option<PathBuf>,
        companion_paths: Option<Vec<PathBuf>>,
        sidecar_paths: Option<Vec<PathBuf>>,
    ) -> Self {
        let category = crate::organizer::resolve_category_for_decision(decision);

        let (assigned_species, scientific_name, routing_tier) = match decision.routing_tier {
            RoutingTier::NoBirdDetected => {
                let sentinel = if decision.assigned_species_slug().contains("Subject")
                    || decision.assigned_species_slug().contains("subject")
                {
                    RoutingTier::SENTINEL_NO_SUBJECT_DETECTED.to_string()
                } else {
                    RoutingTier::SENTINEL_NO_BIRD_DETECTED.to_string()
                };
                (sentinel, None, RoutingTier::NoBirdDetected)
            }
            _ => {
                if category == AssetCategory::NoSubjectDetected
                    && !crate::types::is_known_bird_slug(decision.assigned_species_slug())
                {
                    (
                        RoutingTier::SENTINEL_NO_SUBJECT_DETECTED.to_string(),
                        None,
                        RoutingTier::NoBirdDetected,
                    )
                } else if decision.is_user_override {
                    let cleaned = decision.assigned_species_slug().replace('_', " ");
                    (cleaned, None, decision.routing_tier)
                } else if let Some(top) = decision.top_predictions.first() {
                    (
                        top.common_name.clone(),
                        Some(top.scientific_name.clone()),
                        decision.routing_tier,
                    )
                } else {
                    let cleaned = decision.assigned_species_slug().replace('_', " ");
                    (cleaned, None, decision.routing_tier)
                }
            }
        };

        let capture_date = bundle.capture_timestamp_ms.and_then(|ts| {
            chrono::DateTime::from_timestamp_millis(ts).map(|dt| dt.format("%Y-%m-%d").to_string())
        });

        let location_source = bundle.gps_coords.map(|g| format!("{:?}", g.source));
        let subdirectory = bundle.relative_path.as_ref().and_then(|p| {
            p.parent().and_then(|parent| {
                let s = parent.to_string_lossy().to_string();
                if s.is_empty() || s == "." {
                    None
                } else {
                    Some(s)
                }
            })
        });

        Self {
            bundle_id: bundle.bundle_id.clone(),
            original_stem: bundle.original_stem.clone(),
            current_path: current_primary_path.unwrap_or_else(|| bundle.primary_image.clone()),
            primary_image: bundle.primary_image.clone(),
            companion_images: companion_paths.unwrap_or_else(|| bundle.companion_images.clone()),
            sidecar_files: sidecar_paths.unwrap_or_else(|| bundle.sidecar_files.clone()),
            relative_preview_path: None,
            is_local: None,
            assigned_species,
            scientific_name,
            routing_tier,
            confidence: decision.final_confidence,
            capture_date,
            location_name: bundle.resolved_location_name.clone(),
            location_source,
            subdirectory,
            top_predictions: decision.top_predictions.clone(),
            category,
        }
    }
}

/// Statistics payload embedded into the client-side catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CatalogStats {
    pub total_photos: usize,
    pub species_count: usize,
    pub confirmed_count: usize,
    pub needs_review_count: usize,
    pub no_bird_count: usize,
    #[serde(default)]
    pub bird_count: usize,
    #[serde(default)]
    pub mammalia_count: usize,
    #[serde(default)]
    pub insecta_count: usize,
    #[serde(default)]
    pub reptilia_count: usize,
    #[serde(default)]
    pub amphibia_count: usize,
    #[serde(default)]
    pub scenery_count: usize,
    #[serde(default)]
    pub architecture_count: usize,
    #[serde(default)]
    pub portrait_count: usize,
    #[serde(default)]
    pub abstract_count: usize,
    #[serde(default)]
    pub no_subject_count: usize,
}

/// Full JSON data structure embedded inside `<script id="catalog-data">`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPayload {
    pub trip_name: String,
    pub mode: String,
    pub catalog_dir: String,
    pub trip_location_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<String>,
    pub generated_at: String,
    pub stats: CatalogStats,
    pub items: Vec<CatalogPayloadItem>,
    pub species_list: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_taxa: Vec<CuratedTaxonEntry>,
}

/// JSON representation of an individual item in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPayloadItem {
    pub id: String,
    pub original_stem: String,
    pub current_path: String,
    pub primary_filename: String,
    pub preview_url: String,
    pub local_src: String,
    pub thumb_src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded_thumb_data_uri: Option<String>,
    pub is_local: bool,
    pub assigned_species: String,
    pub scientific_name: Option<String>,
    pub routing_tier: String,
    pub confidence: f32,
    pub capture_date: Option<String>,
    pub location_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdirectory: Option<String>,
    pub companion_files: Vec<String>,
    pub sidecar_files: Vec<String>,
    pub top_predictions: Vec<PayloadPrediction>,
    #[serde(default)]
    pub category: String,
    /// Archive-relative path of the primary asset for unambiguous CLI command generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

/// Serializable species prediction candidate for the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadPrediction {
    pub common_name: String,
    pub scientific_name: String,
    pub score: f32,
}

/// Generates a standalone, zero-dependency offline HTML photo catalog.
///
/// # Arguments
/// * `trip_name` - Descriptive title for the photo archive catalog.
/// * `items` - Slice of catalog items to include in the visual gallery.
/// * `mode` - Active organization mode (`InPlace`, `InPlaceRename`, or `HierarchicalHardlinkHierarchy`).
/// * `output_path` - Target destination path for `catalog.html`.
///
/// # Returns
/// `Ok(())` on successful catalog creation, or an `anyhow::Result` error if output directories
/// cannot be created or catalog template generation fails.
///
/// # Errors
/// Returns an error if filesystem directory creation fails or temporary file writes fail.
pub fn generate_html_catalog(
    trip_name: &str,
    items: &[CatalogItem],
    mode: OrganizationMode,
    output_path: &Path,
) -> Result<()> {
    let output_root = output_path.parent().unwrap_or(Path::new("."));
    if !output_root.as_os_str().is_empty() && !output_root.exists() {
        fs::create_dir_all(output_root).with_context(|| {
            format!(
                "Failed to create catalog directory: {}",
                output_root.display()
            )
        })?;
    }

    // Check if a manifest database exists in output_root/.avicull/manifest.sqlite
    let db_path = output_root.join(".avicull/manifest.sqlite");
    let manifest_db = if db_path.exists() {
        crate::db::ManifestDb::open_or_create(&db_path).ok()
    } else {
        None
    };

    let cached_thumbnails = manifest_db
        .as_ref()
        .and_then(|db| db.get_all_thumbnails().ok())
        .unwrap_or_default();

    // Pre-scan existing previews directory once upfront to collapse 1000s of NFS stat RPCs into 1 readdir call
    let previews_dir = output_root.join(".avicull").join("previews");
    let _ = fs::create_dir_all(&previews_dir);
    let existing_previews: HashSet<String> = if previews_dir.is_dir() {
        fs::read_dir(&previews_dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    } else {
        HashSet::new()
    };

    let payload_results: Vec<(CatalogPayloadItem, Option<(String, String)>)> = items
        .par_iter()
        .map(|item| {
            build_catalog_payload_item(item, output_root, &existing_previews, &cached_thumbnails)
        })
        .collect();

    let mut payload_items = Vec::with_capacity(payload_results.len());
    let mut new_thumbnails_to_persist = Vec::new();

    for (payload_item, new_thumb) in payload_results {
        payload_items.push(payload_item);
        if let Some(thumb) = new_thumb {
            new_thumbnails_to_persist.push(thumb);
        }
    }

    if !new_thumbnails_to_persist.is_empty() {
        if let Some(ref db) = manifest_db {
            let slice: Vec<(&str, &str)> = new_thumbnails_to_persist
                .iter()
                .map(|(b, u)| (b.as_str(), u.as_str()))
                .collect();
            let _ = db.store_thumbnails_bulk(&slice);
        }
    }

    let mut confirmed_count = 0;
    let mut needs_review_count = 0;
    let mut no_bird_count = 0;
    let mut unique_species = HashSet::new();

    let mut bird_count = 0;
    let mut mammalia_count = 0;
    let mut insecta_count = 0;
    let mut reptilia_count = 0;
    let mut amphibia_count = 0;
    let mut scenery_count = 0;
    let mut architecture_count = 0;
    let mut portrait_count = 0;
    let mut abstract_count = 0;
    let mut no_subject_count = 0;

    for item in items {
        let is_bird = item.category == AssetCategory::Bird;
        match item.routing_tier {
            RoutingTier::ConfirmedSpecies => {
                confirmed_count += 1;
                if is_bird
                    && !item.assigned_species.starts_with('_')
                    && !item.assigned_species.is_empty()
                {
                    unique_species.insert(item.assigned_species.clone());
                }
            }
            RoutingTier::NeedsReview => {
                needs_review_count += 1;
                if is_bird
                    && !item.assigned_species.starts_with('_')
                    && !item.assigned_species.is_empty()
                {
                    unique_species.insert(item.assigned_species.clone());
                }
            }
            RoutingTier::NoBirdDetected => {
                no_bird_count += 1;
            }
        }

        match item.category {
            AssetCategory::Bird => bird_count += 1,
            AssetCategory::Mammalia => mammalia_count += 1,
            AssetCategory::Insecta => insecta_count += 1,
            AssetCategory::Reptilia => reptilia_count += 1,
            AssetCategory::Amphibia => amphibia_count += 1,
            AssetCategory::Scenery => scenery_count += 1,
            AssetCategory::Architecture => architecture_count += 1,
            AssetCategory::Portrait => portrait_count += 1,
            AssetCategory::Abstract => abstract_count += 1,
            AssetCategory::NoSubjectDetected => no_subject_count += 1,
            AssetCategory::Other(_) => {}
        }
    }

    let stats = CatalogStats {
        total_photos: items.len(),
        species_count: unique_species.len(),
        confirmed_count,
        needs_review_count,
        no_bird_count,
        bird_count,
        mammalia_count,
        insecta_count,
        reptilia_count,
        amphibia_count,
        scenery_count,
        architecture_count,
        portrait_count,
        abstract_count,
        no_subject_count,
    };

    let mut species_list = load_species_taxonomy_list();
    species_list.push("No Bird Detected".to_string());
    for item in items {
        if item.category == AssetCategory::Bird
            && !item.assigned_species.starts_with('_')
            && !item.assigned_species.is_empty()
        {
            species_list.push(item.assigned_species.clone());
            for pred in &item.top_predictions {
                if !pred.common_name.is_empty() {
                    species_list.push(pred.common_name.clone());
                }
            }
        }
    }
    species_list.sort();
    species_list.dedup();

    let mode_str = match mode {
        OrganizationMode::HierarchicalHardlinkHierarchy => "hierarchy",
        OrganizationMode::InPlaceRename => "in-place-rename",
        OrganizationMode::InPlace => "in-place",
    };

    let catalog_dir_str = output_root
        .canonicalize()
        .unwrap_or_else(|_| output_root.to_path_buf())
        .to_string_lossy()
        .to_string();

    let trip_location_name = items.iter().find_map(|i| i.location_name.clone());

    // Collect all unique directories and ancestor subdirectories
    let mut dir_set = HashSet::new();
    for item in items {
        if let Some(ref subdir) = item.subdirectory {
            let path = Path::new(subdir);
            let mut current = Some(path);
            while let Some(p) = current {
                let s = p.to_string_lossy().to_string();
                if !s.is_empty() && s != "." {
                    dir_set.insert(s);
                }
                current = p.parent();
            }
        }
    }
    let mut directories: Vec<String> = dir_set.into_iter().collect();
    directories.sort();

    let payload = CatalogPayload {
        trip_name: trip_name.to_string(),
        mode: mode_str.to_string(),
        catalog_dir: catalog_dir_str,
        trip_location_name,
        directories,
        generated_at: Utc::now().to_rfc3339(),
        stats,
        items: payload_items,
        species_list,
        multi_taxa: load_curated_multi_taxa(),
    };

    let html_content = template::render_catalog_html(&payload);

    // Write via a temporary file in standard temporary storage first, then copy to destination.
    // Cloud storage fileprovider mounts (e.g. Proton Drive / iCloud Drive) can deadlock
    // in kernel open() if direct O_TRUNC/O_CREAT is held by file coordination daemons.
    let temp_file = tempfile::Builder::new()
        .prefix("bird_catalog_")
        .suffix(".tmp")
        .tempfile_in(std::env::temp_dir())
        .with_context(|| "Failed to create temporary file for catalog HTML")?;
    fs::write(temp_file.path(), html_content)
        .with_context(|| "Failed to write catalog HTML to temporary file")?;

    if output_path.exists() {
        let _ = fs::remove_file(output_path);
    }
    fs::copy(temp_file.path(), output_path)
        .with_context(|| format!("Failed to copy catalog HTML to {}", output_path.display()))?;

    // Maintain backwards-compatible 'catalog.html' symlink/alias alongside 'bird_catalog.html' (and vice versa)
    let filename = output_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    if filename == "bird_catalog.html" {
        let alias_path = output_root.join("catalog.html");
        let _ = fs::remove_file(&alias_path);
        #[cfg(unix)]
        {
            if std::os::unix::fs::symlink("bird_catalog.html", &alias_path).is_err() {
                let _ = fs::copy(output_path, &alias_path);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = fs::copy(output_path, &alias_path);
        }
    } else if filename == "catalog.html" {
        let alias_path = output_root.join("bird_catalog.html");
        let _ = fs::remove_file(&alias_path);
        #[cfg(unix)]
        {
            if std::os::unix::fs::symlink("catalog.html", &alias_path).is_err() {
                let _ = fs::copy(output_path, &alias_path);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = fs::copy(output_path, &alias_path);
        }
    }

    Ok(())
}

/// Standard RFC 4648 Base64 encoding using pure bit operations (zero external dependencies).
pub fn encode_base64(bytes: &[u8]) -> String {
    const STANDARD_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        match chunk.len() {
            3 => {
                let n =
                    (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
                encoded.push(STANDARD_CHARS[((n >> 18) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 12) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 6) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[(n & 63) as usize] as char);
            }
            2 => {
                let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8);
                encoded.push(STANDARD_CHARS[((n >> 18) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 12) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 6) & 63) as usize] as char);
                encoded.push('=');
            }
            1 => {
                let n = u32::from(chunk[0]) << 16;
                encoded.push(STANDARD_CHARS[((n >> 18) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 12) & 63) as usize] as char);
                encoded.push('=');
                encoded.push('=');
            }
            _ => unreachable!(),
        }
    }

    encoded
}

/// Resolves the preview file name and target path inside `<output_root>/.avicull/previews/`.
fn preview_path_and_name(output_root: &Path, original_stem: &str) -> (String, PathBuf) {
    let preview_name = format!("{original_stem}.jpg");
    let target_file = output_root
        .join(".avicull")
        .join("previews")
        .join(&preview_name);
    (preview_name, target_file)
}

/// Constructs a single `CatalogPayloadItem` and optional newly generated micro-thumbnail data URI.
fn build_catalog_payload_item(
    item: &CatalogItem,
    output_root: &Path,
    existing_previews: &HashSet<String>,
    cached_thumbnails: &std::collections::HashMap<String, String>,
) -> (CatalogPayloadItem, Option<(String, String)>) {
    let active_primary = item.active_primary_path();

    let is_local = item.is_local.unwrap_or_else(|| {
        if crate::geo::is_dataless_file(active_primary) {
            false
        } else {
            active_primary.exists()
        }
    });

    let preview_url =
        resolve_or_extract_preview(item, output_root, Some(existing_previews), is_local);
    let persistent_thumb = ensure_persistent_thumbnail_with_previews(
        item,
        output_root,
        Some(existing_previews),
        is_local,
    );
    let thumb_src =
        persistent_thumb.unwrap_or_else(|| format!(".avicull/previews/{}.jpg", item.original_stem));

    let (embedded_thumb_data_uri, new_thumb_entry) =
        if let Some(cached_uri) = cached_thumbnails.get(&item.bundle_id) {
            (Some(cached_uri.clone()), None)
        } else if item
            .relative_preview_path
            .as_deref()
            .is_some_and(|p| p.starts_with("data:image/"))
        {
            (item.relative_preview_path.clone(), None)
        } else if let Some(fresh_uri) = generate_base64_micro_thumbnail_with_previews(
            item,
            output_root,
            Some(existing_previews),
            is_local,
        ) {
            (
                Some(fresh_uri.clone()),
                Some((item.bundle_id.clone(), fresh_uri)),
            )
        } else {
            (None, None)
        };

    let current_path_rel = to_relative_string(&item.current_path, output_root);
    let local_src = current_path_rel.clone();

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

    let relative_path = item
        .subdirectory
        .as_ref()
        .map(|sub| format!("{sub}/{primary_filename}"));

    let payload_item = CatalogPayloadItem {
        id: item.bundle_id.clone(),
        original_stem: item.original_stem.clone(),
        current_path: current_path_rel,
        primary_filename,
        preview_url,
        local_src,
        thumb_src,
        embedded_thumb_data_uri,
        is_local,
        assigned_species: item.assigned_species.clone(),
        scientific_name: item.scientific_name.clone(),
        routing_tier: tier_str.to_string(),
        confidence: item.confidence,
        capture_date: item.capture_date.clone(),
        location_name: item.location_name.clone(),
        location_source: item.location_source.clone(),
        subdirectory: item.subdirectory.clone(),
        companion_files,
        sidecar_files,
        top_predictions: top_preds,
        category: item.category.as_str().to_string(),
        relative_path,
    };

    (payload_item, new_thumb_entry)
}

/// Generates a lightweight base64 micro-thumbnail data URI (max dimension ~300px, quality 68)
/// embedded directly into the HTML catalog JSON payload so the gallery renders standalone
/// without requiring local synchronization of `.avicull/`.
///
/// # Arguments
/// * `item` - Catalog item containing asset paths and metadata.
/// * `output_root` - Root destination directory containing `.avicull/previews/`.
///
/// # Returns
/// `Some(data_uri)` containing the base64-encoded micro-thumbnail, or `None` if unavailable.
///
/// # Invariants
/// Strictly avoids reading dataless or cloud-evicted image payloads.
pub fn generate_base64_micro_thumbnail(item: &CatalogItem, output_root: &Path) -> Option<String> {
    generate_base64_micro_thumbnail_with_previews(
        item,
        output_root,
        None,
        item.is_local.unwrap_or(true),
    )
}

fn generate_base64_micro_thumbnail_with_previews(
    item: &CatalogItem,
    output_root: &Path,
    existing_previews: Option<&HashSet<String>>,
    is_local: bool,
) -> Option<String> {
    // 1. Try reading an existing persistent preview thumbnail if present on disk
    let (preview_name, preview_path) = preview_path_and_name(output_root, &item.original_stem);

    let preview_exists =
        existing_previews.map_or_else(|| preview_path.exists(), |set| set.contains(&preview_name));

    let preview_bytes_opt = if preview_exists {
        fs::read(&preview_path).ok()
    } else {
        None
    };

    // If cloud-evicted or dataless and no preview is cached, do not eagerly hydrate
    if preview_bytes_opt.is_none() && !is_local {
        return None;
    }

    // 2. If no persistent preview, try extracting embedded JPEG from RAW, or opening image
    let (raw_image_bytes, candidate_path) = if let Some(bytes) = preview_bytes_opt {
        (Some(bytes), None)
    } else {
        let active_primary = if item.current_path.exists() {
            &item.current_path
        } else {
            &item.primary_image
        };

        if active_primary.exists() {
            if let Ok(file_bytes) = fs::read(active_primary) {
                if let Some(preview_bytes) = extract_embedded_jpeg_preview(&file_bytes) {
                    (Some(preview_bytes), None)
                } else {
                    // Check companion or readable image path
                    let candidate = if let Some(comp) = item.companion_images.first() {
                        if comp.exists() {
                            Some(comp.as_path())
                        } else {
                            None
                        }
                    } else {
                        let is_web = active_primary
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| {
                                matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp")
                            });
                        if is_web {
                            Some(active_primary.as_path())
                        } else {
                            None
                        }
                    };
                    (None, candidate)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    };

    // Now convert or downsample image into a compact JPEG byte buffer
    let micro_jpeg_bytes = if let Some(bytes) = raw_image_bytes {
        // If the bytes are already a compact JPEG (under 25KB), check dimension or use directly
        if bytes.len() <= 25 * 1024 {
            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                if dyn_img.width() <= 320 && dyn_img.height() <= 320 {
                    bytes
                } else {
                    downsample_to_micro_jpeg(&dyn_img)?
                }
            } else {
                bytes
            }
        } else if let Ok(dyn_img) = image::load_from_memory(&bytes) {
            downsample_to_micro_jpeg(&dyn_img)?
        } else {
            return None;
        }
    } else {
        let src_path = candidate_path?;
        if let Ok(dyn_img) = image::open(src_path) {
            downsample_to_micro_jpeg(&dyn_img)?
        } else {
            return None;
        }
    };

    let b64 = encode_base64(&micro_jpeg_bytes);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

fn downsample_to_micro_jpeg(img: &image::DynamicImage) -> Option<Vec<u8>> {
    let thumb = img.thumbnail(300, 300);
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 68);
    encoder.encode_image(&thumb).ok()?;
    Some(buf)
}

/// Ensures a persistent web-displayable preview JPEG exists on disk inside
/// `<output_root>/.avicull/previews/<original_stem>.jpg`.
///
/// Extracts the embedded JPEG preview from camera RAW files or generates an 800×800
/// thumbnail from companion/web images if no preview currently exists.
///
/// # Arguments
/// * `item` - Catalog item containing asset paths and metadata.
/// * `output_root` - Root destination directory containing `.avicull/previews/`.
///
/// # Returns
/// `Some(path)` relative to `output_root` if the persistent preview exists or was created,
/// or `None` if extraction/generation was not possible.
///
/// # Invariants
/// Guaranteed non-panicking and strictly read-only with respect to primary source images.
pub fn ensure_persistent_thumbnail(item: &CatalogItem, output_root: &Path) -> Option<String> {
    ensure_persistent_thumbnail_with_previews(
        item,
        output_root,
        None,
        item.is_local.unwrap_or(true),
    )
}

fn ensure_persistent_thumbnail_with_previews(
    item: &CatalogItem,
    output_root: &Path,
    existing_previews: Option<&HashSet<String>>,
    is_local: bool,
) -> Option<String> {
    let (preview_name, target_file) = preview_path_and_name(output_root, &item.original_stem);

    let already_exists =
        existing_previews.map_or_else(|| target_file.exists(), |set| set.contains(&preview_name));

    if already_exists {
        return Some(format!(".avicull/previews/{preview_name}"));
    }

    if !is_local {
        return None;
    }

    let active_primary = item.active_primary_path();

    let mut generated = false;
    let previews_dir = output_root.join(".avicull").join("previews");

    // A. If standalone camera RAW file: extract embedded JPEG preview thumbnail
    if active_primary.exists() {
        if let Ok(raw_bytes) = fs::read(active_primary) {
            if let Some(preview_bytes) = extract_embedded_jpeg_preview(&raw_bytes) {
                if fs::create_dir_all(&previews_dir).is_ok()
                    && fs::write(&target_file, &preview_bytes).is_ok()
                {
                    generated = true;
                }
            }
        }
    }

    // B. If not generated from RAW, try companion JPEG or readable web image
    if !generated {
        let candidate_img = if let Some(comp) = item.companion_images.first() {
            if comp.exists() {
                Some(comp.as_path())
            } else {
                None
            }
        } else if active_primary.exists() {
            let is_web = active_primary
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp")
                });
            if is_web {
                Some(active_primary)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(src) = candidate_img {
            if let Ok(img) = image::open(src) {
                let thumb = img.thumbnail(800, 800);
                if fs::create_dir_all(&previews_dir).is_ok() {
                    let mut buf = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut buf);
                    if thumb
                        .write_to(&mut cursor, image::ImageFormat::Jpeg)
                        .is_ok()
                        && fs::write(&target_file, &buf).is_ok()
                    {
                        generated = true;
                    }
                }
            }
        }
    }

    if generated || target_file.exists() {
        Some(format!(".avicull/previews/{preview_name}"))
    } else {
        None
    }
}

/// Resolves or extracts preview thumbnail path for an item relative to output_root.
fn resolve_or_extract_preview(
    item: &CatalogItem,
    output_root: &Path,
    existing_previews: Option<&HashSet<String>>,
    is_local: bool,
) -> String {
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

    let active_primary = item.active_primary_path();

    // 3. Web-renderable primary image
    let is_web_img = active_primary
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            let lower = e.to_lowercase();
            matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp")
        });

    if is_web_img && active_primary.exists() {
        return to_relative_string(active_primary, output_root);
    }

    // 4. Standalone camera RAW file: check if precomputed preview already exists on disk
    let (preview_name, target_file) = preview_path_and_name(output_root, &item.original_stem);

    let preview_exists =
        existing_previews.map_or_else(|| target_file.exists(), |set| set.contains(&preview_name));

    if preview_exists {
        return format!(".avicull/previews/{preview_name}");
    }

    // If cloud-evicted or non-local, do NOT eagerly read raw bytes
    if !is_local {
        return to_relative_string(active_primary, output_root);
    }

    // Extract embedded JPEG preview thumbnail from camera RAW file
    if active_primary.exists() {
        if let Ok(raw_bytes) = fs::read(active_primary) {
            if let Some(preview_bytes) = extract_embedded_jpeg_preview(&raw_bytes) {
                let previews_dir = output_root.join(".avicull").join("previews");
                if fs::create_dir_all(&previews_dir).is_ok()
                    && fs::write(&target_file, &preview_bytes).is_ok()
                {
                    return format!(".avicull/previews/{preview_name}");
                }
            }
        }
    }

    // 5. Fallback relative path (or string representation) for onerror browser fallback
    to_relative_string(active_primary, output_root)
}

/// Formats a path relative to root when possible.
fn to_relative_string(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
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
                        let slug = obj
                            .get("folder_slug")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if crate::types::is_known_bird_slug(slug) {
                            if let Some(name) = obj.get("common_name").and_then(|v| v.as_str()) {
                                list.push(name.to_string());
                            }
                        }
                    }
                    if !list.is_empty() {
                        return list;
                    }
                }
            }
        }
    }

    // Default to authoritative embedded canonical eBird taxonomy
    let canonical = crate::types::canonical_bird_common_names();
    if !canonical.is_empty() {
        return canonical.to_vec();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Classification, GpsCoordinate, LocationSource, RoutingTier, SpeciesPrediction,
    };

    #[test]
    fn test_catalog_item_default() {
        let item = CatalogItem::default();
        assert_eq!(item.bundle_id, "");
        assert_eq!(item.assigned_species, "");
        assert_eq!(item.routing_tier, RoutingTier::ConfirmedSpecies);
        assert_eq!(item.confidence, 0.0);
        assert_eq!(item.companion_images, &[] as &[PathBuf]);
        assert_eq!(item.sidecar_files, &[] as &[PathBuf]);
    }

    #[test]
    fn test_catalog_item_from_bundle_and_decision_confirmed() {
        let bundle = AssetBundle {
            bundle_id: "test-bundle-1".to_string(),
            source_inode: 12345,
            trip_folder_name: "Leh".to_string(),
            original_stem: "IMG_1001".to_string(),
            primary_image: PathBuf::from("IMG_1001.CR2"),
            companion_images: vec![PathBuf::from("IMG_1001.JPG")],
            sidecar_files: vec![PathBuf::from("IMG_1001.CR2.xmp")],
            capture_timestamp_ms: Some(1_730_000_000_000), // ~2024-10-27
            gps_coords: Some(GpsCoordinate {
                latitude: 34.1526,
                longitude: 77.5771,
                source: LocationSource::ExifGps,
            }),
            resolved_location_name: Some("Leh, Ladakh".to_string()),
            relative_path: Some(PathBuf::from("trips/subfolder/IMG_1001.CR2")),
        };

        let decision = BundleDecision {
            bundle_id: "test-bundle-1".to_string(),
            burst_id: None,
            detection_confidence: 0.98,
            bbox_xyxy_norm: None,
            top_predictions: vec![SpeciesPrediction {
                taxon_id: "houspa".to_string(),
                common_name: "House Sparrow".to_string(),
                folder_slug: "House_Sparrow".to_string(),
                scientific_name: "Passer domesticus".to_string(),
                visual_prob: 0.95,
                geo_prior: 0.90,
                combined_score: 0.93,
            }],
            frame_confidence: 0.93,
            final_confidence: 0.93,
            classification: Classification::granular(AssetCategory::Bird, "House_Sparrow"),
            routing_tier: RoutingTier::ConfirmedSpecies,
            is_user_override: false,
        };

        let item = CatalogItem::from_bundle_and_decision(&bundle, &decision, None, None, None);
        assert_eq!(item.assigned_species, "House Sparrow");
        assert_eq!(item.scientific_name.as_deref(), Some("Passer domesticus"));
        assert_eq!(item.routing_tier, RoutingTier::ConfirmedSpecies);
        assert_eq!(item.location_name.as_deref(), Some("Leh, Ladakh"));
        assert_eq!(item.location_source.as_deref(), Some("ExifGps"));
        assert_eq!(item.subdirectory.as_deref(), Some("trips/subfolder"));
        assert!(item.capture_date.is_some());
    }

    #[test]
    fn test_catalog_item_from_bundle_and_decision_no_bird() {
        let bundle = AssetBundle {
            bundle_id: "test-bundle-2".to_string(),
            source_inode: 12346,
            trip_folder_name: "Leh".to_string(),
            original_stem: "IMG_1002".to_string(),
            primary_image: PathBuf::from("IMG_1002.CR2"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: None,
        };

        let decision = BundleDecision {
            bundle_id: "test-bundle-2".to_string(),
            burst_id: None,
            detection_confidence: 0.1,
            bbox_xyxy_norm: None,
            top_predictions: vec![],
            frame_confidence: 0.1,
            final_confidence: 0.1,
            classification: Classification::from_slug(RoutingTier::SENTINEL_NO_BIRD_DETECTED),
            routing_tier: RoutingTier::NoBirdDetected,
            is_user_override: false,
        };

        let item = CatalogItem::from_bundle_and_decision(&bundle, &decision, None, None, None);
        assert_eq!(item.assigned_species, "_No_Bird_Detected");
        assert_eq!(item.scientific_name, None);
        assert_eq!(item.routing_tier, RoutingTier::NoBirdDetected);
        assert_eq!(item.subdirectory, None);
    }

    #[test]
    fn test_catalog_item_from_bundle_and_decision_user_override() {
        let bundle = AssetBundle {
            bundle_id: "test-bundle-3".to_string(),
            source_inode: 12347,
            trip_folder_name: "Leh".to_string(),
            original_stem: "IMG_1003".to_string(),
            primary_image: PathBuf::from("IMG_1003.CR2"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: None,
        };

        let decision = BundleDecision {
            bundle_id: "test-bundle-3".to_string(),
            burst_id: None,
            detection_confidence: 0.8,
            bbox_xyxy_norm: None,
            top_predictions: vec![],
            frame_confidence: 0.8,
            final_confidence: 0.8,
            classification: Classification::granular(AssetCategory::Bird, "Eurasian_Tree_Sparrow"),
            routing_tier: RoutingTier::ConfirmedSpecies,
            is_user_override: true,
        };

        let item = CatalogItem::from_bundle_and_decision(&bundle, &decision, None, None, None);
        assert_eq!(item.assigned_species, "Eurasian Tree Sparrow");
        assert_eq!(item.scientific_name, None);
    }

    #[test]
    fn test_fallback_curated_species_contains_common_birds() {
        let species = fallback_curated_species();
        assert!(species.contains(&"House Sparrow".to_string()));
        assert!(species.contains(&"Black Redstart".to_string()));
        assert!(species.contains(&"Common Kingfisher".to_string()));
    }
}
