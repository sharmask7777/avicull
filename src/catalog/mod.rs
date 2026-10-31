pub mod template;

use crate::scanner::extract_embedded_jpeg_preview;
use crate::storage::StorageBackend;
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
        let (assigned_species, scientific_name) = match decision.routing_tier {
            RoutingTier::NoBirdDetected => ("_No_Bird_Detected".to_string(), None),
            _ => {
                if decision.is_user_override {
                    let cleaned = decision.assigned_species_slug.replace('_', " ");
                    (cleaned, None)
                } else if let Some(top) = decision.top_predictions.first() {
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
            routing_tier: decision.routing_tier,
            confidence: decision.final_confidence,
            capture_date,
            location_name: bundle.resolved_location_name.clone(),
            location_source,
            subdirectory,
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
    pub trip_location_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<String>,
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

    // Check if a manifest database exists in output_root/.avicull/manifest.sqlite
    let db_path = output_root.join(".avicull/manifest.sqlite");
    let manifest_db = if db_path.exists() {
        crate::db::ManifestDb::open_or_create(&db_path).ok()
    } else {
        None
    };

    let mut cached_thumbnails = manifest_db
        .as_ref()
        .and_then(|db| db.get_all_thumbnails().ok())
        .unwrap_or_default();

    let mut new_thumbnails_to_persist = Vec::new();

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
        let persistent_thumb = ensure_persistent_thumbnail(item, output_root);
        let thumb_src = persistent_thumb
            .unwrap_or_else(|| format!(".avicull/previews/{}.jpg", item.original_stem));

        let embedded_thumb_data_uri =
            if let Some(cached_uri) = cached_thumbnails.get(&item.bundle_id) {
                Some(cached_uri.clone())
            } else if let Some(fresh_uri) = generate_base64_micro_thumbnail(item, output_root) {
                new_thumbnails_to_persist.push((item.bundle_id.clone(), fresh_uri.clone()));
                cached_thumbnails.insert(item.bundle_id.clone(), fresh_uri.clone());
                Some(fresh_uri)
            } else {
                None
            };

        let current_path_rel = to_relative_string(&item.current_path, output_root);
        let local_src = current_path_rel.clone();
        let is_local = item.is_local.unwrap_or_else(|| {
            if item.current_path.exists() {
                crate::storage::LocalVolumeBackend
                    .inspect_locality(&item.current_path)
                    .map(|loc| loc.pre_existing_local)
                    .unwrap_or(true)
            } else {
                false
            }
        });

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
        });
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

    let stats = CatalogStats {
        total_photos: items.len(),
        species_count: unique_species.len(),
        confirmed_count,
        needs_review_count,
        no_bird_count,
    };

    let mut species_list = load_species_taxonomy_list();
    species_list.push("No Bird Detected".to_string());
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
        .with_context(|| format!("Failed to copy catalog HTML to {:?}", output_path))?;

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
                let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
                encoded.push(STANDARD_CHARS[((n >> 18) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 12) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 6) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[(n & 63) as usize] as char);
            }
            2 => {
                let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8);
                encoded.push(STANDARD_CHARS[((n >> 18) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 12) & 63) as usize] as char);
                encoded.push(STANDARD_CHARS[((n >> 6) & 63) as usize] as char);
                encoded.push('=');
            }
            1 => {
                let n = (chunk[0] as u32) << 16;
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

/// Generates a lightweight base64 micro-thumbnail data URI (max dimension ~300px, quality ~65)
/// embedded directly into the HTML catalog JSON payload so the gallery renders standalone
/// without requiring local synchronization of `.avicull/`.
pub fn generate_base64_micro_thumbnail(item: &CatalogItem, output_root: &Path) -> Option<String> {
    // 1. Try reading an existing persistent preview thumbnail if present on disk
    let previews_dir = output_root.join(".avicull").join("previews");
    let preview_path = previews_dir.join(format!("{}.jpg", item.original_stem));

    let preview_bytes_opt = if preview_path.exists() {
        fs::read(&preview_path).ok()
    } else {
        None
    };

    // 2. If no persistent preview, try extracting embedded JPEG from RAW, or opening image
    let (raw_image_bytes, candidate_path) = match preview_bytes_opt {
        Some(bytes) => (Some(bytes), None),
        None => {
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
                                .map(|e| {
                                    matches!(
                                        e.to_lowercase().as_str(),
                                        "jpg" | "jpeg" | "png" | "webp"
                                    )
                                })
                                .unwrap_or(false);
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
    Some(format!("data:image/jpeg;base64,{}", b64))
}

fn downsample_to_micro_jpeg(img: &image::DynamicImage) -> Option<Vec<u8>> {
    let thumb = img.thumbnail(300, 300);
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 68);
    encoder.encode_image(&thumb).ok()?;
    Some(buf)
}

/// Resolves or extracts preview thumbnail path for an item relative to output_root.
pub fn ensure_persistent_thumbnail(item: &CatalogItem, output_root: &Path) -> Option<String> {
    let previews_dir = output_root.join(".avicull").join("previews");
    let preview_name = format!("{}.jpg", item.original_stem);
    let target_file = previews_dir.join(&preview_name);

    if target_file.exists() {
        return Some(format!(".avicull/previews/{}", preview_name));
    }

    let active_primary = if item.current_path.exists() {
        &item.current_path
    } else {
        &item.primary_image
    };

    let mut generated = false;

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
                .map(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp"))
                .unwrap_or(false);
            if is_web {
                Some(active_primary.as_path())
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
        Some(format!(".avicull/previews/{}", preview_name))
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GpsCoordinate, LocationSource, RoutingTier, SpeciesPrediction};

    #[test]
    fn test_catalog_item_default() {
        let item = CatalogItem::default();
        assert_eq!(item.bundle_id, "");
        assert_eq!(item.assigned_species, "");
        assert_eq!(item.routing_tier, RoutingTier::ConfirmedSpecies);
        assert_eq!(item.confidence, 0.0);
        assert!(item.companion_images.is_empty());
        assert!(item.sidecar_files.is_empty());
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
            capture_timestamp_ms: Some(1730000000000), // ~2024-10-27
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
            assigned_species_slug: "House_Sparrow".to_string(),
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
            assigned_species_slug: "_No_Bird_Detected".to_string(),
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
            assigned_species_slug: "Eurasian_Tree_Sparrow".to_string(),
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
