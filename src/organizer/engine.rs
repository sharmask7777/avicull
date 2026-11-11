//! # Reusable Organize Pipeline Engine
//!
//! Orchestrates the multi-stage photo organization pipeline:
//! 1. Mode and backend validation (local APFS vs cloud/network shares).
//! 2. Manifest initialization and SQLite child manifest discovery.
//! 3. Input tree scanning and metadata reconciliation.
//! 4. EXIF GPS resolution, gazetteer matching, and regional checklist prior assignment.
//! 5. Partitioning into cached vs new/reclassified bundles.
//! 6. Batched neural detection and species classification with cloud cache eviction.
//! 7. Temporal burst consensus pooling and user manual override preservation.
//! 8. POSIX hard link materialization, in-place non-destructive cataloging, or in-place renaming.
//! 9. Standalone HTML photo catalog generation.

use crate::burst::{smooth_burst_consensus, ClassifiedBundleItem};
use crate::catalog::{generate_html_catalog, CatalogItem, DEFAULT_CATALOG_FILENAME};
use crate::classifier::{MockSpeciesClassifier, OrtSpeciesClassifier, SpeciesClassifier};
use crate::cli::OrganizeArgs;
use crate::db::{ExistingBundleMap, ManifestDb};
use crate::detector::{select_and_pad_bird_crop, BirdDetector, MockBirdDetector, OrtBirdDetector};
use crate::geo::resolve_locations;
use crate::location::gazetteer::GazetteerEngine;
use crate::location::path_parser::extract_candidate_location_tokens;
use crate::location::resolver::resolve_trip_locations;
use crate::organizer::{
    execute_inplace_bundle_rename_with_category, infer_prediction_from_stem,
    materialize_bundle_hardlinks, resolve_category_for_decision,
};
use crate::scanner::{
    extract_embedded_jpeg_preview, scan_input_tree_with_skip, validate_volume_and_disjointness,
};
use crate::types::{
    AssetBundle, AssetCategory, ConfidenceMode, GpsCoordinate, LocationSource, OrganizationMode,
    RoutingTier,
};
use anyhow::{anyhow, Context, Result};
use image::DynamicImage;
use std::fs::File;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Decodes an image file for neural inference, prioritizing companion JPEGs and embedded RAW previews.
///
/// # Arguments
/// * `primary` - Path to the primary asset (e.g. RAW or JPEG).
/// * `companions` - Companion image paths (e.g. camera-generated JPEGs).
///
/// # Returns
/// Decoded dynamic image ready for cropping or embedding extraction.
///
/// # Errors
/// Returns error if the file cannot be read or decoded into an image.
fn load_image_for_inference(primary: &Path, companions: &[PathBuf]) -> Result<DynamicImage> {
    // Priority 1: Check companion images for JPEG for ultra-fast decoding
    for comp in companions {
        let ext = comp
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            if let Ok(img) = image::open(comp) {
                return Ok(img);
            }
        }
    }

    // Priority 2: If primary is JPEG or common raster format, decode directly
    let prim_ext = primary
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if prim_ext == "jpg"
        || prim_ext == "jpeg"
        || prim_ext == "png"
        || prim_ext == "tif"
        || prim_ext == "tiff"
    {
        if let Ok(img) = image::open(primary) {
            return Ok(img);
        }
    }

    // Priority 3: RAW file — scan for largest embedded JPEG preview
    if let Ok(mut f) = File::open(primary) {
        let mut buffer = Vec::new();
        let _ = f.read_to_end(&mut buffer);
        if let Some(jpeg_bytes) = extract_embedded_jpeg_preview(&buffer) {
            if let Ok(img) = image::load_from_memory(&jpeg_bytes) {
                return Ok(img);
            }
        }
    }

    // Fallback: Attempt standard image::open
    image::open(primary).with_context(|| format!("Failed to decode image {}", primary.display()))
}

/// Prepares an image path for the batch inference subprocess.
///
/// For RAW files lacking a companion JPEG, extracts an embedded preview into a temporary directory.
fn prepare_image_for_inference(bundle: &AssetBundle, temp_dir: &Path, idx: usize) -> PathBuf {
    for comp in &bundle.companion_images {
        let ext = comp
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            return comp.clone();
        }
    }

    let prim_ext = bundle
        .primary_image
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if prim_ext == "jpg" || prim_ext == "jpeg" || prim_ext == "png" {
        return bundle.primary_image.clone();
    }

    // Try extracting embedded preview for RAW
    if let Ok(mut f) = File::open(&bundle.primary_image) {
        let mut buffer = Vec::new();
        if f.read_to_end(&mut buffer).is_ok() {
            if let Some(preview_bytes) = extract_embedded_jpeg_preview(&buffer) {
                let out_path = temp_dir.join(format!("preview_{idx}.jpg"));
                if std::fs::write(&out_path, preview_bytes).is_ok() {
                    return out_path;
                }
            }
        }
    }

    bundle.primary_image.clone()
}

#[derive(serde::Serialize)]
struct BatchTask {
    bundle_id: String,
    primary_image: PathBuf,
    trip_folder_name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    region_code: String,
}

#[derive(serde::Deserialize)]
struct InferResultItem {
    bundle_id: String,
    detection_confidence: f32,
    top_predictions: Vec<crate::types::SpeciesPrediction>,
    embedding: Vec<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    category: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    assigned_species_slug: Option<String>,
}

/// Executes batch neural detection and species classification via Python subprocess runner.
fn run_batch_inference(
    bundles: &[&AssetBundle],
    models_dir: &Path,
    min_detect: f32,
    geo_prior_weight: f32,
    bundle_regions: &std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, ClassifiedBundleItem>> {
    let infer_script = crate::classifier::resolve_inference_script()?;
    let embeddings_file = models_dir.join("species_embeddings.safetensors");

    if !infer_script.exists() || !embeddings_file.exists() {
        return Err(anyhow!("Real models or infer.py script not found"));
    }

    let temp_dir = tempfile::tempdir()?;
    let batch_file = temp_dir.path().join("batch_tasks.json");
    let output_file = temp_dir.path().join("batch_results.json");

    let mut tasks = Vec::with_capacity(bundles.len());

    for (idx, bundle) in bundles.iter().enumerate() {
        let img_path = prepare_image_for_inference(bundle, temp_dir.path(), idx);
        let (lat, lon) = match bundle.gps_coords {
            Some(coords) => (Some(coords.latitude), Some(coords.longitude)),
            None => (None, None),
        };
        let region = bundle_regions
            .get(&bundle.bundle_id)
            .cloned()
            .unwrap_or_default();

        tasks.push(BatchTask {
            bundle_id: bundle.bundle_id.clone(),
            primary_image: img_path,
            trip_folder_name: bundle.trip_folder_name.clone(),
            latitude: lat,
            longitude: lon,
            region_code: region,
        });
    }

    std::fs::write(&batch_file, serde_json::to_string(&tasks)?)?;

    let batch_file_str = batch_file.to_string_lossy();
    let output_file_str = output_file.to_string_lossy();
    let infer_script_str = infer_script.to_string_lossy();

    let mut child = std::process::Command::new("uv")
        .args([
            "run",
            "--with",
            "torch,open-clip-torch,safetensors,ultralytics,pillow",
            "python3",
            infer_script_str.as_ref(),
            "--models-dir",
            models_dir.to_str().unwrap_or("models"),
            "--batch-file",
            batch_file_str.as_ref(),
            "--output-file",
            output_file_str.as_ref(),
            "--min-detect",
            &min_detect.to_string(),
            "--geo-prior-weight",
            &geo_prior_weight.to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let mut err_lines = Vec::new();
    if let Some(stderr) = child.stderr.take() {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(std::result::Result::ok) {
            eprintln!("{line}");
            err_lines.push(line);
        }
    }

    let status = child.wait()?;
    if !status.success() {
        let err_msg = err_lines.join("\n");
        return Err(anyhow!("Python inference runner failed: {err_msg}"));
    }

    let results_data = std::fs::read(&output_file)?;
    let parsed: Vec<InferResultItem> = serde_json::from_slice(&results_data)?;

    let mut map = std::collections::HashMap::new();
    let bundle_map: std::collections::HashMap<_, _> =
        bundles.iter().map(|b| (&b.bundle_id, *b)).collect();

    for item in parsed {
        if let Some(bundle) = bundle_map.get(&item.bundle_id) {
            let top_predictions = item.top_predictions;
            let detection_confidence = item.detection_confidence;

            let posteriors: Vec<(String, f32)> = top_predictions
                .iter()
                .map(|p| (p.taxon_id.clone(), p.combined_score))
                .collect();

            map.insert(
                item.bundle_id.clone(),
                ClassifiedBundleItem {
                    bundle_id: item.bundle_id,
                    trip_folder: bundle.trip_folder_name.clone(),
                    capture_timestamp_ms: bundle.capture_timestamp_ms,
                    detection_confidence,
                    bbox_norm: None,
                    embedding: item.embedding,
                    top_predictions,
                    posteriors,
                },
            );
        }
    }

    Ok(map)
}

/// Fallback in-process parallel classification using Rayon.
fn run_fallback_parallel_classification(
    new_bundles: &[&AssetBundle],
    detector: &std::sync::Arc<Box<dyn BirdDetector>>,
    classifier: &std::sync::Arc<Box<dyn SpeciesClassifier>>,
    min_detect: f32,
    geo_prior_weight: f32,
    bundle_regions: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, ClassifiedBundleItem> {
    use rayon::prelude::*;
    let total = new_bundles.len();
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let interval = if total <= 20 {
        5
    } else if total <= 100 {
        10
    } else {
        25
    };

    new_bundles
        .par_iter()
        .map(|bundle| {
            let img_res = load_image_for_inference(&bundle.primary_image, &bundle.companion_images);
            let (crop_res, det_conf, bbox) = match img_res {
                Ok(img) => {
                    let detections = detector.detect(&img).unwrap_or_default();
                    match select_and_pad_bird_crop(&img, &detections, min_detect, (224, 224)) {
                        Some((cropped, b, conf)) => (Some(cropped), conf, Some(b)),
                        None => (None, 0.0, None),
                    }
                }
                Err(_) => (None, 0.0, None),
            };

            let item = if let Some(cropped) = crop_res {
                let region = bundle_regions
                    .get(&bundle.bundle_id)
                    .map(std::string::String::as_str);
                let (emb, preds) = classifier
                    .classify_with_region(&cropped, bundle.gps_coords, region, geo_prior_weight)
                    .unwrap_or_else(|_| (Vec::new(), vec![]));

                if preds.is_empty() {
                    if let Some((_cat, pred, _tier)) =
                        infer_prediction_from_stem(&bundle.original_stem)
                    {
                        let posteriors = vec![(pred.taxon_id.clone(), pred.combined_score)];
                        ClassifiedBundleItem {
                            bundle_id: bundle.bundle_id.clone(),
                            trip_folder: bundle.trip_folder_name.clone(),
                            capture_timestamp_ms: bundle.capture_timestamp_ms,
                            detection_confidence: 0.90,
                            bbox_norm: bbox,
                            embedding: emb,
                            top_predictions: vec![pred],
                            posteriors,
                        }
                    } else {
                        ClassifiedBundleItem {
                            bundle_id: bundle.bundle_id.clone(),
                            trip_folder: bundle.trip_folder_name.clone(),
                            capture_timestamp_ms: bundle.capture_timestamp_ms,
                            detection_confidence: det_conf,
                            bbox_norm: bbox,
                            embedding: emb,
                            top_predictions: preds,
                            posteriors: vec![],
                        }
                    }
                } else {
                    let posteriors: Vec<(String, f32)> = preds
                        .iter()
                        .map(|p| (p.taxon_id.clone(), p.combined_score))
                        .collect();

                    ClassifiedBundleItem {
                        bundle_id: bundle.bundle_id.clone(),
                        trip_folder: bundle.trip_folder_name.clone(),
                        capture_timestamp_ms: bundle.capture_timestamp_ms,
                        detection_confidence: det_conf,
                        bbox_norm: bbox,
                        embedding: emb,
                        top_predictions: preds,
                        posteriors,
                    }
                }
            } else if let Some((_cat, pred, _tier)) =
                infer_prediction_from_stem(&bundle.original_stem)
            {
                let posteriors = vec![(pred.taxon_id.clone(), pred.combined_score)];
                let mut fallback_emb = vec![0.0f32; 512];
                fallback_emb[0] = 1.0;
                ClassifiedBundleItem {
                    bundle_id: bundle.bundle_id.clone(),
                    trip_folder: bundle.trip_folder_name.clone(),
                    capture_timestamp_ms: bundle.capture_timestamp_ms,
                    detection_confidence: 0.90,
                    bbox_norm: None,
                    embedding: fallback_emb,
                    top_predictions: vec![pred],
                    posteriors,
                }
            } else {
                ClassifiedBundleItem {
                    bundle_id: bundle.bundle_id.clone(),
                    trip_folder: bundle.trip_folder_name.clone(),
                    capture_timestamp_ms: bundle.capture_timestamp_ms,
                    detection_confidence: 0.0,
                    bbox_norm: None,
                    embedding: Vec::new(),
                    top_predictions: vec![],
                    posteriors: vec![],
                }
            };

            let count = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if count.is_multiple_of(interval) || count == total {
                let pct = (count * 100).checked_div(total).unwrap_or(100);
                eprintln!(
                    "[classification] Fallback progress: {count}/{total} ({pct}%) bundles processed..."
                );
            }

            (bundle.bundle_id.clone(), item)
        })
        .collect()
}

/// Resolves the models directory from user flag, local workspace, or global cache, prompting if interactive.
fn resolve_or_prompt_models_dir(
    requested: Option<PathBuf>,
    no_prompt: bool,
) -> Result<Option<PathBuf>> {
    let resolved = if let Some(r) = requested {
        Some(r)
    } else {
        // Check local ./models directory
        let local_models = PathBuf::from("models");
        if local_models.join("species_embeddings.safetensors").exists() {
            Some(local_models)
        } else {
            // Check persistent user cache ~/.cache/avicull/models
            let home = std::env::var("HOME").ok();
            let cache_dir = home.map(|h| PathBuf::from(h).join(".cache/avicull/models"));
            if let Some(ref c_dir) = cache_dir {
                if c_dir.join("species_embeddings.safetensors").exists() {
                    let manifest_path = c_dir.join("manifest.json");
                    if let Ok(meta) = std::fs::metadata(&manifest_path) {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed.as_secs() > 30 * 86400 {
                                    println!(
                                        "[avicull] Note: Model cache is over 30 days old. You can update the eBird taxonomy anytime."
                                    );
                                }
                            }
                        }
                    }
                    Some(c_dir.clone())
                } else if !no_prompt && std::io::stdin().is_terminal() {
                    // Prompt user in interactive terminal sessions
                    use std::io::{self, Write};
                    print!(
                        "\n[avicull] Avian neural weights and 11,000-species eBird taxonomy (~30 MB) not found in cache.\nWould you like to download & initialize them into '{}' now? [Y/n]: ",
                        c_dir.display()
                    );
                    let _ = io::stdout().flush();

                    let mut input = String::new();
                    if io::stdin().read_line(&mut input).is_ok() {
                        let choice = input.trim().to_lowercase();
                        if choice.is_empty() || choice == "y" || choice == "yes" {
                            println!(
                                "[avicull] Initializing eBird taxonomy & BioCLIP embeddings on Apple Silicon GPU..."
                            );
                            std::fs::create_dir_all(c_dir)?;
                            let c_dir_str = c_dir.to_string_lossy();
                            let status_res = std::process::Command::new("uv")
                                .args([
                                    "run",
                                    "--with",
                                    "torch,open-clip-torch,safetensors,ultralytics",
                                    "python3",
                                    "scripts/build_taxonomy_embeddings.py",
                                    c_dir_str.as_ref(),
                                ])
                                .status();

                            match status_res {
                                Ok(status) if status.success() => {
                                    println!(
                                        "[avicull] ✓ Models successfully installed to '{}' and ready for reuse across all future runs.",
                                        c_dir.display()
                                    );
                                    Some(c_dir.clone())
                                }
                                _ => {
                                    eprintln!(
                                        "[avicull] Warning: Failed to download models. Falling back to mock classifier."
                                    );
                                    None
                                }
                            }
                        } else {
                            println!(
                                "[avicull] Skipping model download. Falling back to mock classifier."
                            );
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    if let Some(ref m_dir) = resolved {
        crate::classifier::sync_regional_checklists(m_dir)?;
    }

    Ok(resolved)
}

/// Executes the full organize pipeline for a single target directory.
///
/// # Arguments
/// * `args` - Configuration parameters defining input paths, organization mode, confidence thresholds, and threads.
///
/// # Returns
/// `Ok(())` upon successful completion of scanning, classification, linking/renaming, and catalog generation.
///
/// # Errors
/// Returns error if volume disjointness checks fail, filesystem I/O fails, or SQLite operations fail.
pub fn run_organize(args: &OrganizeArgs) -> Result<()> {
    println!("== avicull organize ==");
    if let Some(t) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build_global()
            .ok();
    }

    let storage_backend = crate::storage::detect_storage_backend(&args.input);
    crate::organizer::validate_organization_mode_for_backend(storage_backend.as_ref(), args.mode)?;

    let is_inplace = matches!(
        args.mode,
        OrganizationMode::InPlace | OrganizationMode::InPlaceRename
    );

    let (operating_root, db_path, skip_dir) = if is_inplace {
        if args.output.is_some() {
            println!(
                "[avicull] Note: In in-place mode, photos and manifests are organized directly in '--input'; '--output' is ignored."
            );
        }
        let db_p = args.input.join(".avicull/manifest.sqlite");
        (args.input.clone(), db_p, None)
    } else {
        let output_dir = args
            .output
            .clone()
            .unwrap_or_else(|| args.input.join("birds_hierarchy"));
        validate_volume_and_disjointness(&args.input, &output_dir)?;
        let db_p = output_dir.join(".avicull/manifest.sqlite");
        (output_dir.clone(), db_p, Some(output_dir))
    };

    let db = ManifestDb::open_or_create(&db_path)?;

    // Discover and import child manifests from subdirectories if any exist
    if let Ok(child_manifests) = crate::scanner::discover_child_manifests(&args.input) {
        for (child_path, child_rel_dir) in child_manifests {
            if let Ok(imported_count) = db.import_child_manifest(&child_path, &child_rel_dir) {
                if imported_count > 0 {
                    println!(
                        "Inheritance: Imported {} bundle(s) from child subdirectory manifest '{}'.",
                        imported_count,
                        child_rel_dir.display()
                    );
                }
            }
        }
    }

    let mut existing_bundle_map = if args.force {
        ExistingBundleMap::new()
    } else {
        db.get_existing_bundle_map().unwrap_or_default()
    };

    let mut _reconciled_moves_count = 0;
    if args.mode == OrganizationMode::InPlaceRename
        && operating_root.exists()
        && !existing_bundle_map.is_empty()
    {
        match crate::organizer::reconcile_inplace_manual_renames(
            &operating_root,
            &db,
            &mut existing_bundle_map,
        ) {
            Ok(count) => {
                _reconciled_moves_count = count;
                if count > 0 {
                    println!(
                        "Reconciliation: Reconciled {count} manual in-place rename(s) into durable SQLite overrides."
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to reconcile manual in-place renames: {e}");
            }
        }
    }

    println!("Scanning source directory '{}'...", args.input.display());
    let mut all_discovered = scan_input_tree_with_skip(&args.input, skip_dir.as_deref())?;
    for bundle in &mut all_discovered {
        if let Some((_, old_bundle)) = existing_bundle_map.get_by_bundle(bundle) {
            bundle.bundle_id.clone_from(&old_bundle.bundle_id);
            bundle.original_stem = old_bundle.original_stem.clone();
            if bundle.gps_coords.is_none() {
                bundle.gps_coords = old_bundle.gps_coords;
            }
            if bundle.capture_timestamp_ms.is_none() {
                bundle.capture_timestamp_ms = old_bundle.capture_timestamp_ms;
            }
            if bundle.resolved_location_name.is_none() {
                bundle.resolved_location_name = old_bundle.resolved_location_name.clone();
            }
        }
    }
    println!(
        "Discovered {} total photo bundles in source.",
        all_discovered.len()
    );

    // Prune missing/deleted bundles from manifest so catalog and state accurately reflect surviving files
    if !existing_bundle_map.is_empty() {
        match crate::organizer::prune_missing_bundles_from_manifest(
            &all_discovered,
            &db,
            &mut existing_bundle_map,
        ) {
            Ok(pruned) if pruned > 0 => {
                println!("Reconciliation: Pruned {pruned} deleted bundle(s) from SQLite manifest.");
            }
            Err(e) => {
                eprintln!("Warning: Failed to prune missing bundles from manifest: {e}");
            }
            _ => {}
        }
    }

    println!(
        "Resolving EXIF metadata and GPS coordinates across {} bundles...",
        all_discovered.len()
    );
    let def_coords = match (args.default_lat, args.default_lon) {
        (Some(la), Some(lo)) => Some((la, lo)),
        _ => None,
    };
    resolve_locations(
        &mut all_discovered,
        args.locations_file.as_deref(),
        def_coords,
    )?;
    let with_gps_count = all_discovered
        .iter()
        .filter(|b| b.gps_coords.is_some())
        .count();
    if args.verbose || with_gps_count > 0 {
        println!(
            "✓ Resolved GPS coordinates for {}/{} bundles.",
            with_gps_count,
            all_discovered.len()
        );
    }

    // Location resolution via Gazetteer and Path Inference for bundles lacking EXIF GPS
    let mut bundle_regions: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let gazetteer_engine = GazetteerEngine::load_embedded()?;

    // Group trips where bundles lack EXIF GPS, or all trips if global --location is specified
    let mut trip_map: std::collections::BTreeMap<String, Vec<&AssetBundle>> =
        std::collections::BTreeMap::new();
    for bundle in &all_discovered {
        let lacks_exif = bundle
            .gps_coords
            .is_none_or(|c| c.source != LocationSource::ExifGps);
        if lacks_exif || args.location.is_some() {
            trip_map
                .entry(bundle.trip_folder_name.clone())
                .or_default()
                .push(bundle);
        }
    }

    if !trip_map.is_empty() || args.location.is_some() {
        let mut trips_to_tokens = Vec::new();
        for (trip_name, b_list) in &trip_map {
            let mut tokens = Vec::new();
            for b in b_list {
                for t in extract_candidate_location_tokens(&b.primary_image) {
                    if !tokens.contains(&t) {
                        tokens.push(t);
                    }
                }
            }
            for t in extract_candidate_location_tokens(Path::new(trip_name)) {
                if !tokens.contains(&t) {
                    tokens.push(t);
                }
            }
            trips_to_tokens.push((trip_name.clone(), tokens));
        }

        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout();
        let is_interactive = !args.no_prompt && std::io::stdin().is_terminal();

        let resolved = resolve_trip_locations(
            &trips_to_tokens,
            &gazetteer_engine,
            &db,
            &mut stdin,
            &mut stdout,
            is_interactive,
            args.location.as_deref(),
        )?;

        // Apply strict 4-tier location precedence:
        // 1. Embedded EXIF GPS (immutable)
        // 2. Explicit image-level override (bundle_locations table)
        // 3. Hierarchical subdirectory/parent override (directory_locations table)
        // 4. Resolved trip-level gazetteer match
        for bundle in &mut all_discovered {
            let _ = crate::location::resolver::apply_location_precedence(bundle, &db);

            if bundle.gps_coords.is_none() {
                if let Some(loc) = resolved.get(&bundle.trip_folder_name) {
                    bundle.resolved_location_name = Some(loc.name.clone());
                    bundle.gps_coords = Some(GpsCoordinate {
                        latitude: loc.latitude,
                        longitude: loc.longitude,
                        source: LocationSource::TripFolderGazetteer,
                    });
                    if !loc.region_code.is_empty() {
                        bundle_regions.insert(bundle.bundle_id.clone(), loc.region_code.clone());
                    }
                }
            } else if bundle.resolved_location_name.is_some() {
                // If region_code exists in db, record it
                if let Ok(Some(cached_loc)) = db.get_bundle_location(&bundle.bundle_id) {
                    if !cached_loc.region_code.is_empty() {
                        bundle_regions.insert(bundle.bundle_id.clone(), cached_loc.region_code);
                    }
                } else if let Ok(Some(cached_loc)) = db.get_hierarchical_directory_location(
                    bundle
                        .relative_path
                        .as_deref()
                        .unwrap_or(&bundle.primary_image),
                ) {
                    if !cached_loc.region_code.is_empty() {
                        bundle_regions.insert(bundle.bundle_id.clone(), cached_loc.region_code);
                    }
                }
            }
        }
    }

    // Ensure any remaining bundles have precedence applied
    for bundle in &mut all_discovered {
        let _ = crate::location::resolver::apply_location_precedence(bundle, &db);
        if !bundle_regions.contains_key(&bundle.bundle_id) {
            if let Ok(Some(loc)) = db.get_bundle_location(&bundle.bundle_id) {
                if !loc.region_code.is_empty() {
                    bundle_regions.insert(bundle.bundle_id.clone(), loc.region_code);
                }
            } else if let Ok(Some(loc)) = db.get_hierarchical_directory_location(
                bundle
                    .relative_path
                    .as_deref()
                    .unwrap_or(&bundle.primary_image),
            ) {
                if !loc.region_code.is_empty() {
                    bundle_regions.insert(bundle.bundle_id.clone(), loc.region_code);
                }
            } else if let Ok(Some(loc)) = db.get_directory_location(&bundle.trip_folder_name) {
                if !loc.region_code.is_empty() {
                    bundle_regions.insert(bundle.bundle_id.clone(), loc.region_code);
                }
            } else if let Some(coords) = bundle.gps_coords {
                // Infer nearest granular region from raw GPS coordinates within 300km territorial cutoff
                if let Some((nearest_entry, _dist_km)) =
                    gazetteer_engine.find_nearest(coords.latitude, coords.longitude, Some(300.0))
                {
                    if !nearest_entry.region_code.is_empty() {
                        bundle_regions
                            .insert(bundle.bundle_id.clone(), nearest_entry.region_code.clone());
                    }
                }
            }
        }
    }

    // Reconcile any manual filesystem moves in the output directory (hierarchy mode)
    if args.mode == OrganizationMode::HierarchicalHardlinkHierarchy
        && operating_root.exists()
        && !existing_bundle_map.is_empty()
    {
        match crate::organizer::reconcile_manual_disk_moves(
            &operating_root,
            &db,
            &mut existing_bundle_map,
        ) {
            Ok(count) => {
                _reconciled_moves_count += count;
                if count > 0 {
                    println!(
                        "Reconciliation: Reconciled {count} manual filesystem move(s) into durable SQLite overrides."
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to reconcile manual filesystem moves: {e}");
            }
        }
    }

    // Partition into new vs cached bundles using bundle_id, source_inode, relative_path, or portable key
    let mut new_bundles = Vec::new();
    let mut cached_items = std::collections::HashMap::new();

    for bundle in &all_discovered {
        if let Some((existing_decision, old_bundle)) = existing_bundle_map.get_by_bundle(bundle) {
            if crate::organizer::should_reclassify_bundle(
                bundle,
                existing_decision,
                old_bundle,
                args.reclassify,
            ) {
                new_bundles.push(bundle);
            } else {
                let posteriors: Vec<(String, f32)> = existing_decision
                    .top_predictions
                    .iter()
                    .map(|p| (p.taxon_id.clone(), p.combined_score))
                    .collect();

                cached_items.insert(
                    bundle.bundle_id.clone(),
                    ClassifiedBundleItem {
                        bundle_id: bundle.bundle_id.clone(),
                        trip_folder: bundle.trip_folder_name.clone(),
                        capture_timestamp_ms: bundle.capture_timestamp_ms,
                        detection_confidence: existing_decision.detection_confidence,
                        bbox_norm: existing_decision.bbox_xyxy_norm,
                        embedding: Vec::new(),
                        top_predictions: existing_decision.top_predictions.clone(),
                        posteriors,
                    },
                );
            }
        } else {
            new_bundles.push(bundle);
        }
    }

    if !existing_bundle_map.is_empty() {
        println!(
            "Manifest status: {} bundles already cached, {} new/reclassified bundles detected to process.",
            cached_items.len(),
            new_bundles.len()
        );
    }

    let latest_mode = db.get_latest_run_mode().unwrap_or(None);
    let mode_matches = match latest_mode {
        Some(m) => m == args.mode,
        None => true,
    };

    if !existing_bundle_map.is_empty() && new_bundles.is_empty() && mode_matches {
        let trip_name = args
            .input
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Avian Photos");
        println!(
            "Generating standalone offline HTML photo catalog at '{}'...",
            operating_root.join(DEFAULT_CATALOG_FILENAME).display()
        );
        crate::organizer::regenerate_catalog_from_manifest(
            &db,
            &operating_root,
            Some(trip_name),
            Some(args.mode),
        )?;

        let unclassified_count = cached_items
            .values()
            .filter(|item| item.detection_confidence == 0.0 && item.top_predictions.is_empty())
            .count();
        if unclassified_count > 0 && !args.reclassify {
            println!(
                "Tip: {unclassified_count} cached photos have no bird detected. Pass `--reclassify` to re-run neural detection and classification."
            );
        }

        println!(
            "✓ All {} photo bundles are already up to date in '{}'.",
            all_discovered.len(),
            operating_root.display()
        );
        return Ok(());
    }

    let models_dir = resolve_or_prompt_models_dir(args.models_dir.clone(), args.no_prompt)?;

    let conf_mode: ConfidenceMode = args.confidence_mode.parse().map_err(|e| anyhow!("{e}"))?;

    // Initialize models only if we have new bundles to process
    let detector: Box<dyn BirdDetector> = if let Some(ref m_dir) = models_dir {
        let yolo_path = m_dir.join("yolo11m.onnx");
        if yolo_path.exists() {
            Box::new(OrtBirdDetector::new(&yolo_path)?)
        } else {
            Box::new(MockBirdDetector::new(vec![]))
        }
    } else {
        Box::new(MockBirdDetector::new(vec![]))
    };

    let classifier: Box<dyn SpeciesClassifier> = if let Some(ref m_dir) = models_dir {
        Box::new(OrtSpeciesClassifier::new(m_dir)?)
    } else {
        Box::new(MockSpeciesClassifier::new(vec![]))
    };

    let detector = Arc::new(detector);
    let classifier = Arc::new(classifier);

    let max_cache_bytes = crate::organizer::chunk::parse_size_budget(&args.max_cache_size)?;
    let new_bundles_owned: Vec<AssetBundle> = new_bundles.iter().map(|&b| b.clone()).collect();
    let chunks = crate::organizer::chunk::partition_into_chunks(
        new_bundles_owned,
        max_cache_bytes,
        storage_backend.as_ref(),
    );
    let total_chunks = chunks.len();

    let mut new_classified = std::collections::HashMap::new();
    let mut total_space_saved_bytes = 0u64;
    let mut bundle_dataless_map: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut failed_bundle_ids = Vec::new();

    let run_id = format!("run-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let mode_str = match args.mode {
        OrganizationMode::InPlace => "in_place",
        OrganizationMode::InPlaceRename => "in_place_rename",
        OrganizationMode::HierarchicalHardlinkHierarchy => "hierarchy",
    };
    let config_json = serde_json::to_string(&serde_json::json!({
        "mode": mode_str,
        "min_confidence": args.min_confidence,
        "confidence_mode": args.confidence_mode,
    }))?;
    db.record_run(
        &run_id,
        &args.input,
        &operating_root,
        args.dry_run,
        &config_json,
    )?;

    for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
        println!(
            "Processing chunk {}/{} ({} bundles, cache budget: {})...",
            chunk_idx + 1,
            total_chunks,
            chunk.len(),
            args.max_cache_size
        );

        // Materialize dataless cloud files with retry resilience and record locality for later eviction
        let outcome = crate::organizer::materialize_chunk_with_retry(
            &chunk,
            storage_backend.as_ref(),
            &db,
            &run_id,
            std::time::Duration::from_millis(1500),
        );

        total_space_saved_bytes += outcome.space_saved_bytes;
        for (b, was_dl) in outcome
            .successful_bundles
            .iter()
            .zip(outcome.was_dataless_flags.iter())
        {
            bundle_dataless_map.insert(b.bundle_id.clone(), *was_dl);
        }
        failed_bundle_ids.extend(outcome.failed_bundle_ids);

        let processable_chunk = outcome.successful_bundles;
        if processable_chunk.is_empty() {
            continue;
        }

        // Inspect locality for eviction after classification
        let localities: Vec<_> = processable_chunk
            .iter()
            .map(|b| {
                storage_backend
                    .inspect_locality(&b.primary_image)
                    .unwrap_or(crate::storage::FileLocality {
                        is_dataless: false,
                        pre_existing_local: true,
                        file_size_bytes: 0,
                    })
            })
            .collect();

        // Run classification on chunk
        let chunk_refs: Vec<&AssetBundle> = processable_chunk.iter().collect();
        let chunk_classified = if let Some(ref m_dir) = models_dir {
            let has_infer_script =
                crate::classifier::resolve_inference_script().is_ok_and(|p| p.is_file());
            if m_dir.join("species_embeddings.safetensors").exists() && has_infer_script {
                match run_batch_inference(
                    &chunk_refs,
                    m_dir,
                    args.min_detect,
                    args.geo_prior_weight,
                    &bundle_regions,
                ) {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("Warning: Real inference error ({e}), using fallback: {e}");
                        run_fallback_parallel_classification(
                            &chunk_refs,
                            &detector,
                            &classifier,
                            args.min_detect,
                            args.geo_prior_weight,
                            &bundle_regions,
                        )
                    }
                }
            } else {
                run_fallback_parallel_classification(
                    &chunk_refs,
                    &detector,
                    &classifier,
                    args.min_detect,
                    args.geo_prior_weight,
                    &bundle_regions,
                )
            }
        } else {
            run_fallback_parallel_classification(
                &chunk_refs,
                &detector,
                &classifier,
                args.min_detect,
                args.geo_prior_weight,
                &bundle_regions,
            )
        };

        // Extract embedded preview for RAW formats if companion missing
        let previews_dir = operating_root.join(".avicull").join("previews");
        let raw_bundles_to_extract: Vec<_> = processable_chunk
            .iter()
            .filter(|b| {
                let is_web = b
                    .primary_image
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| {
                        matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp")
                    });
                !is_web && b.companion_images.is_empty()
            })
            .collect();

        if !raw_bundles_to_extract.is_empty() {
            let _ = std::fs::create_dir_all(&previews_dir);
            let raw_total = raw_bundles_to_extract.len();
            if args.verbose || raw_total > 5 {
                println!(
                    "  Extracting embedded preview JPEGs for {raw_total} RAW photo bundle(s)..."
                );
            }
            for (idx, bundle) in raw_bundles_to_extract.iter().enumerate() {
                let preview_path = previews_dir.join(format!("{}.jpg", bundle.original_stem));
                if !preview_path.exists() {
                    if let Ok(raw_bytes) = std::fs::read(&bundle.primary_image) {
                        if let Some(preview_bytes) = extract_embedded_jpeg_preview(&raw_bytes) {
                            let _ = std::fs::write(&preview_path, preview_bytes);
                        }
                    }
                }
                if args.verbose && ((idx + 1).is_multiple_of(50) || (idx + 1) == raw_total) {
                    println!("    Extracted previews: {}/{}...", idx + 1, raw_total);
                }
            }
        }

        // Evict downloaded cloud files for this chunk to honor max cache budget
        for (bundle, loc) in processable_chunk.iter().zip(localities.iter()) {
            let _ = storage_backend.evict_if_downloaded(&bundle.primary_image, loc);
        }

        new_classified.extend(chunk_classified);
    }

    // Filter all_discovered to exclude any bundles that failed to download
    let processable_discovered: Vec<AssetBundle> = all_discovered
        .into_iter()
        .filter(|b| !failed_bundle_ids.contains(&b.bundle_id))
        .collect();

    // Assemble all classified items in processable_discovered order
    let mut all_classified_items = Vec::with_capacity(processable_discovered.len());
    for bundle in &processable_discovered {
        if let Some(item) = new_classified.get(&bundle.bundle_id) {
            all_classified_items.push(item.clone());
        } else if let Some(item) = cached_items.get(&bundle.bundle_id) {
            all_classified_items.push(item.clone());
        }
    }

    println!("Applying burst clustering and consensus smoothing...");
    let mut decisions = smooth_burst_consensus(
        &all_classified_items,
        args.burst_window_ms,
        0.80,
        conf_mode,
        args.min_confidence,
    );
    let unique_bursts = decisions
        .iter()
        .filter_map(|d| d.burst_id.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .len();
    println!(
        "✓ Formed {} burst clusters across {} photo bundles.",
        unique_bursts,
        decisions.len()
    );

    // Preserve user manual overrides across burst consensus and re-runs (unless --reclassify was passed)
    if !args.reclassify {
        for (bundle, decision) in processable_discovered.iter().zip(decisions.iter_mut()) {
            if let Some((existing_dec, _)) = existing_bundle_map.get_by_bundle(bundle) {
                if existing_dec.is_user_override {
                    decision
                        .classification
                        .clone_from(&existing_dec.classification);
                    decision.routing_tier = existing_dec.routing_tier;
                    decision.final_confidence = existing_dec.final_confidence;
                    decision.is_user_override = true;
                }
            }
        }
    }

    let was_dataless_flags: Vec<bool> = processable_discovered
        .iter()
        .map(|b| {
            bundle_dataless_map
                .get(&b.bundle_id)
                .copied()
                .unwrap_or(false)
        })
        .collect();
    db.record_bundles_with_locality(
        &run_id,
        &decisions,
        &processable_discovered,
        &was_dataless_flags,
    )?;

    // Persist bundle decisions with 512-dim visual embeddings and resolved taxonomic categories
    let embeddings_by_id: std::collections::HashMap<&str, &[f32]> = all_classified_items
        .iter()
        .filter(|item| item.embedding.len() == 512 && item.embedding.iter().any(|&v| v != 0.0))
        .map(|item| (item.bundle_id.as_str(), item.embedding.as_slice()))
        .collect();

    for decision in &decisions {
        let emb = embeddings_by_id.get(decision.bundle_id.as_str()).copied();
        let category = resolve_category_for_decision(decision);
        db.insert_bundle_decision(decision, &category, emb)?;
    }

    let mut catalog_items = Vec::with_capacity(processable_discovered.len());
    let mut total_ops_count = 0usize;

    if args.mode == OrganizationMode::InPlace {
        println!(
            "In-place non-destructive cataloging: keeping source files untouched in '--input'..."
        );
        for (bundle, decision) in processable_discovered.iter().zip(decisions.iter()) {
            let cat_item = CatalogItem::from_bundle_and_decision(
                bundle,
                decision,
                Some(bundle.primary_image.clone()),
                Some(bundle.companion_images.clone()),
                Some(bundle.sidecar_files.clone()),
            );
            catalog_items.push(cat_item);
        }
    } else if args.mode == OrganizationMode::InPlaceRename {
        let total_renames = processable_discovered.len();
        println!(
            "Executing in-place file renames for {total_renames} bundles (dry_run: {})...",
            args.dry_run
        );
        for (idx, (bundle, decision)) in processable_discovered
            .iter()
            .zip(decisions.iter())
            .enumerate()
        {
            let renames = if decision.routing_tier == RoutingTier::NoBirdDetected {
                // Bundles where no bird is detected are preserved in-place without renaming.
                Vec::new()
            } else if !args.dry_run {
                let species_name = match decision.routing_tier {
                    RoutingTier::ConfirmedSpecies => {
                        if let Some(top) = decision.top_predictions.first() {
                            top.common_name.clone()
                        } else {
                            decision.assigned_species_slug().to_string()
                        }
                    }
                    RoutingTier::NeedsReview => {
                        if let Some(top) = decision.top_predictions.first() {
                            format!("Needs_Review_{}", top.common_name)
                        } else {
                            format!("Needs_Review_{}", decision.assigned_species_slug())
                        }
                    }
                    RoutingTier::NoBirdDetected => unreachable!(),
                };
                let category_val = resolve_category_for_decision(decision);
                let category_str = if category_val == AssetCategory::NoSubjectDetected {
                    ""
                } else {
                    category_val.as_str()
                };
                execute_inplace_bundle_rename_with_category(
                    bundle,
                    category_str,
                    &species_name,
                    None,
                    None,
                )?
            } else {
                Vec::new()
            };

            for (orig, renamed) in &renames {
                db.record_inplace_rename(&run_id, orig, renamed)?;
            }
            total_ops_count += renames.len();

            if args.verbose || (idx + 1).is_multiple_of(50) || (idx + 1) == total_renames {
                println!(
                    "  Renamed {}/{} bundles ({} operations recorded)...",
                    idx + 1,
                    total_renames,
                    total_ops_count
                );
            }

            let rename_map: std::collections::HashMap<_, _> = renames.into_iter().collect();

            let new_primary = rename_map
                .get(&bundle.primary_image)
                .cloned()
                .unwrap_or_else(|| bundle.primary_image.clone());
            let new_companions: Vec<PathBuf> = bundle
                .companion_images
                .iter()
                .map(|p| rename_map.get(p).cloned().unwrap_or_else(|| p.clone()))
                .collect();
            let new_sidecars: Vec<PathBuf> = bundle
                .sidecar_files
                .iter()
                .map(|p| rename_map.get(p).cloned().unwrap_or_else(|| p.clone()))
                .collect();

            let cat_item = CatalogItem::from_bundle_and_decision(
                bundle,
                decision,
                Some(new_primary),
                Some(new_companions),
                Some(new_sidecars),
            );
            catalog_items.push(cat_item);
        }
        println!("✓ In-place renaming complete: {total_ops_count} operations recorded.");
    } else {
        let total_links = processable_discovered.len();
        println!(
            "Materializing hard links for {total_links} bundles into '{}' (dry_run: {})...",
            operating_root.display(),
            args.dry_run
        );
        let mut total_ops = Vec::new();
        for (idx, (bundle, decision)) in processable_discovered
            .iter()
            .zip(decisions.iter())
            .enumerate()
        {
            let ops = materialize_bundle_hardlinks(
                bundle,
                decision,
                &operating_root,
                &run_id,
                args.dry_run,
            )?;

            let prim_dest = ops
                .iter()
                .find(|op| op.source_path == bundle.primary_image)
                .map(|op| op.dest_path.clone());
            let comp_dests: Vec<PathBuf> = bundle
                .companion_images
                .iter()
                .filter_map(|comp| {
                    ops.iter()
                        .find(|op| op.source_path == *comp)
                        .map(|op| op.dest_path.clone())
                })
                .collect();
            let sidecar_dests: Vec<PathBuf> = bundle
                .sidecar_files
                .iter()
                .filter_map(|sc| {
                    ops.iter()
                        .find(|op| op.source_path == *sc)
                        .map(|op| op.dest_path.clone())
                })
                .collect();

            let cat_item = CatalogItem::from_bundle_and_decision(
                bundle,
                decision,
                prim_dest,
                Some(comp_dests),
                Some(sidecar_dests),
            );
            catalog_items.push(cat_item);

            total_ops.extend(ops);

            if args.verbose || (idx + 1).is_multiple_of(50) || (idx + 1) == total_links {
                println!(
                    "  Hardlinked {}/{} bundles ({} links created)...",
                    idx + 1,
                    total_links,
                    total_ops.len()
                );
            }
        }
        total_ops_count = total_ops.len();
        db.record_hardlink_operations(&total_ops)?;
        println!("✓ Hardlink materialization complete: {total_ops_count} links created.");
    }

    db.complete_run(&run_id)?;

    let catalog_path = operating_root.join(DEFAULT_CATALOG_FILENAME);
    let trip_name = args
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Avian Photos");
    println!(
        "Generating standalone offline HTML photo catalog at '{}'...",
        catalog_path.display()
    );
    generate_html_catalog(trip_name, &catalog_items, args.mode, &catalog_path)?;

    let confirmed = decisions
        .iter()
        .filter(|d| d.routing_tier == RoutingTier::ConfirmedSpecies)
        .count();
    let review = decisions
        .iter()
        .filter(|d| d.routing_tier == RoutingTier::NeedsReview)
        .count();
    let no_bird = decisions
        .iter()
        .filter(|d| d.routing_tier == RoutingTier::NoBirdDetected)
        .count();

    println!(
        "✓ Done! Confirmed: {confirmed}, Needs Review: {review}, No Bird: {no_bird}. Operations: {total_ops_count}"
    );

    // Summary reporting for processed photos and selective eviction savings
    let processed_count = processable_discovered.len();
    let pending_count = failed_bundle_ids.len();
    let space_saved_gb = total_space_saved_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    if pending_count > 0 || total_space_saved_bytes > 0 {
        println!(
            "Organization complete: {processed_count} photos processed, {pending_count} pending retry."
        );
        println!("Disk space saved via selective eviction: {space_saved_gb:.1} GB.");
        if pending_count > 0 {
            println!(
                "Note: {} photos encountered download timeouts. Re-run 'avicull organize --input \"{}\"' to process pending photos.",
                pending_count,
                args.input.display()
            );
        }
    }

    Ok(())
}
