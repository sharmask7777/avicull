use anyhow::{anyhow, Context, Result};
use avicull::burst::{smooth_burst_consensus, ClassifiedBundleItem};
use avicull::classifier::{MockSpeciesClassifier, OrtSpeciesClassifier, SpeciesClassifier};
use avicull::db::ManifestDb;
use avicull::detector::{
    select_and_pad_bird_crop, BirdDetector, MockBirdDetector, OrtBirdDetector,
};
use avicull::geo::resolve_locations;
use avicull::organizer::{clean_run_hardlinks, materialize_bundle_hardlinks};
use avicull::scanner::{
    extract_embedded_jpeg_preview, scan_input_tree_with_skip, validate_volume_and_disjointness,
};
use avicull::types::{ConfidenceMode, RoutingTier};
use clap::{Args, Parser, Subcommand};
use image::DynamicImage;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "avicull",
    author = "avicull contributors",
    version = "0.1.0",
    about = "Local AI Bird Photo Organizer for macOS Apple Silicon"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify APFS same-volume hardlink compatibility, sidecar bundles & location resolution
    Scan(ScanArgs),

    /// Run detection, classification, burst smoothing, and populate hard-linked species tree
    Organize(OrganizeArgs),

    /// Re-materialize hard links from cached SQLite predictions without re-running ML inference
    Relink(RelinkArgs),

    /// Unlink all generated hard links in --output for a given run_id (leaving --input untouched)
    Clean(CleanArgs),

    /// Export a CSV/JSON summary of species counts and _Needs_Review items
    Report(ReportArgs),
}

#[derive(Args, Debug)]
struct ScanArgs {
    /// Read-only root directory of trip-based bird photos
    #[arg(short, long)]
    input: PathBuf,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Optional TOML file mapping trip folder keywords to {lat, lon}
    #[arg(long)]
    locations_file: Option<PathBuf>,

    /// Fallback default latitude
    #[arg(long)]
    default_lat: Option<f64>,

    /// Fallback default longitude
    #[arg(long)]
    default_lon: Option<f64>,
}

#[derive(Args, Debug)]
struct OrganizeArgs {
    /// Read-only root directory of trip-based bird photos
    #[arg(short, long)]
    input: PathBuf,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Compute classifications and write SQLite plan without creating filesystem hard links
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Threshold for Confirmed vs _Needs_Review (default: 0.65)
    #[arg(long, default_value_t = 0.65)]
    min_confidence: f32,

    /// YOLO11 bird detection confidence floor (default: 0.25)
    #[arg(long, default_value_t = 0.25)]
    min_detect: f32,

    /// Max timestamp gap for burst clustering in ms (default: 3000)
    #[arg(long, default_value_t = 3000)]
    burst_window_ms: i64,

    /// Filename score: [burst-consensus | per-frame] (default: burst-consensus)
    #[arg(long, default_value = "burst-consensus")]
    confidence_mode: String,

    /// Strength of SINR geographical prior [0.0..2.0] (default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    geo_prior_weight: f32,

    /// Optional TOML file mapping trip folder keywords to {lat, lon}
    #[arg(long)]
    locations_file: Option<PathBuf>,

    /// Fallback default latitude
    #[arg(long)]
    default_lat: Option<f64>,

    /// Fallback default longitude
    #[arg(long)]
    default_lon: Option<f64>,

    /// Directory containing exported .onnx and .safetensors files
    #[arg(long)]
    models_dir: Option<PathBuf>,

    /// Rayon worker thread count for I/O and preprocessing
    #[arg(long)]
    threads: Option<usize>,
}

#[derive(Args, Debug)]
struct RelinkArgs {
    /// Target root directory containing .avicull/manifest.sqlite (defaults to ./birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// New threshold for Confirmed vs _Needs_Review (default: 0.65)
    #[arg(long, default_value_t = 0.65)]
    min_confidence: f32,

    /// Specific run_id to relink (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Args, Debug)]
struct CleanArgs {
    /// Target root directory containing .avicull/manifest.sqlite (defaults to ./birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Specific run_id to clean (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Args, Debug)]
struct ReportArgs {
    /// Target root directory containing .avicull/manifest.sqlite (defaults to ./birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Specific run_id to report on (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,

    /// Output format: json or csv (default: json)
    #[arg(long, default_value = "json")]
    format: String,
}

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan(args) => {
            println!("== avicull scan ==");
            let output_dir = args
                .output
                .unwrap_or_else(|| args.input.join("birds_hierarchy"));
            validate_volume_and_disjointness(&args.input, &output_dir)?;
            println!("✓ APFS same-volume and path validation succeeded.");

            let mut bundles = scan_input_tree_with_skip(&args.input, Some(&output_dir))?;
            println!("✓ Discovered {} asset bundles.", bundles.len());

            let def_coords = match (args.default_lat, args.default_lon) {
                (Some(la), Some(lo)) => Some((la, lo)),
                _ => None,
            };
            resolve_locations(&mut bundles, args.locations_file.as_deref(), def_coords)?;

            let with_gps = bundles.iter().filter(|b| b.gps_coords.is_some()).count();
            println!(
                "✓ Resolved GPS coordinates for {}/{} bundles.",
                with_gps,
                bundles.len()
            );

            let total_sidecars: usize = bundles.iter().map(|b| b.sidecar_files.len()).sum();
            let total_companions: usize = bundles.iter().map(|b| b.companion_images.len()).sum();
            println!(
                "✓ Associated {} companion images and {} sidecar files.",
                total_companions, total_sidecars
            );
            println!("Target output: {}", output_dir.display());
            println!("Ready for organize.");
        }

        Commands::Organize(args) => {
            println!("== avicull organize ==");
            if let Some(t) = args.threads {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(t)
                    .build_global()
                    .ok();
            }

            let output_dir = args
                .output
                .unwrap_or_else(|| args.input.join("birds_hierarchy"));
            validate_volume_and_disjointness(&args.input, &output_dir)?;

            let db_path = output_dir.join(".avicull/manifest.sqlite");
            let db = ManifestDb::open_or_create(&db_path)?;
            let existing_bundle_map = db.get_existing_bundle_map().unwrap_or_default();

            let mut all_discovered = scan_input_tree_with_skip(&args.input, Some(&output_dir))?;
            println!(
                "Discovered {} total photo bundles in source.",
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

            // Partition into new vs cached bundles
            let mut new_bundles = Vec::new();
            let mut cached_items = std::collections::HashMap::new();

            for bundle in &all_discovered {
                if let Some((existing_decision, _)) = existing_bundle_map.get(&bundle.bundle_id) {
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
                            embedding: vec![0.0; 512],
                            top_predictions: existing_decision.top_predictions.clone(),
                            posteriors,
                        },
                    );
                } else {
                    new_bundles.push(bundle);
                }
            }

            if !existing_bundle_map.is_empty() {
                println!(
                    "Manifest status: {} bundles already cached, {} new bundles detected to process.",
                    cached_items.len(),
                    new_bundles.len()
                );
            }

            if !existing_bundle_map.is_empty() && new_bundles.is_empty() {
                println!(
                    "✓ All {} photo bundles are already up to date in '{}'.",
                    all_discovered.len(),
                    output_dir.display()
                );
                return Ok(());
            }

            let conf_mode: ConfidenceMode =
                args.confidence_mode.parse().map_err(|e| anyhow!("{}", e))?;

            // Initialize models only if we have new bundles to process
            let detector: Box<dyn BirdDetector> = if let Some(ref m_dir) = args.models_dir {
                let yolo_path = m_dir.join("yolo11m.onnx");
                if yolo_path.exists() {
                    Box::new(OrtBirdDetector::new(&yolo_path)?)
                } else {
                    Box::new(MockBirdDetector::new(vec![]))
                }
            } else {
                Box::new(MockBirdDetector::new(vec![]))
            };

            let classifier: Box<dyn SpeciesClassifier> = if let Some(ref m_dir) = args.models_dir {
                Box::new(OrtSpeciesClassifier::new(m_dir)?)
            } else {
                Box::new(MockSpeciesClassifier::new(vec![]))
            };

            use rayon::prelude::*;
            use std::sync::Arc;

            let detector = Arc::new(detector);
            let classifier = Arc::new(classifier);

            println!(
                "Running detection and classification on {} new bundles in parallel...",
                new_bundles.len()
            );
            let new_classified: std::collections::HashMap<String, ClassifiedBundleItem> =
                new_bundles
                    .par_iter()
                    .map(|bundle| {
                        let img_res = load_image_for_inference(
                            &bundle.primary_image,
                            &bundle.companion_images,
                        );
                        let (crop_res, det_conf, bbox) = match img_res {
                            Ok(img) => {
                                let detections = detector.detect(&img).unwrap_or_default();
                                match select_and_pad_bird_crop(
                                    &img,
                                    &detections,
                                    args.min_detect,
                                    (224, 224),
                                ) {
                                    Some((cropped, b, conf)) => (Some(cropped), conf, Some(b)),
                                    None => (None, 0.0, None),
                                }
                            }
                            Err(_) => (None, 0.0, None),
                        };

                        let item = if let Some(cropped) = crop_res {
                            let (emb, preds) = classifier
                                .classify(&cropped, bundle.gps_coords, args.geo_prior_weight)
                                .unwrap_or_else(|_| (vec![0.0; 512], vec![]));

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
                        } else {
                            // No bird detected
                            ClassifiedBundleItem {
                                bundle_id: bundle.bundle_id.clone(),
                                trip_folder: bundle.trip_folder_name.clone(),
                                capture_timestamp_ms: bundle.capture_timestamp_ms,
                                detection_confidence: 0.0,
                                bbox_norm: None,
                                embedding: vec![0.0; 512],
                                top_predictions: vec![],
                                posteriors: vec![],
                            }
                        };
                        (bundle.bundle_id.clone(), item)
                    })
                    .collect();

            // Assemble all classified items in all_discovered order
            let mut all_classified_items = Vec::with_capacity(all_discovered.len());
            for bundle in &all_discovered {
                if let Some(item) = new_classified.get(&bundle.bundle_id) {
                    all_classified_items.push(item.clone());
                } else if let Some(item) = cached_items.get(&bundle.bundle_id) {
                    all_classified_items.push(item.clone());
                }
            }

            println!("Applying burst clustering and consensus smoothing...");
            let decisions = smooth_burst_consensus(
                &all_classified_items,
                args.burst_window_ms,
                0.80,
                conf_mode,
                args.min_confidence,
            );

            let run_id = format!("run-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
            db.record_run(&run_id, &args.input, &output_dir, args.dry_run, "{}")?;
            db.record_bundles(&run_id, &decisions, &all_discovered)?;

            println!("Materializing hard links (dry_run: {})...", args.dry_run);
            let mut total_ops = Vec::new();
            for (bundle, decision) in all_discovered.iter().zip(decisions.iter()) {
                let ops = materialize_bundle_hardlinks(
                    bundle,
                    decision,
                    &output_dir,
                    &run_id,
                    args.dry_run,
                )?;
                total_ops.extend(ops);
            }
            db.record_hardlink_operations(&total_ops)?;
            db.complete_run(&run_id)?;

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
                "✓ Done! Confirmed: {}, Needs Review: {}, No Bird: {}. Operations: {}",
                confirmed,
                review,
                no_bird,
                total_ops.len()
            );
        }

        Commands::Relink(args) => {
            println!("== avicull relink ==");
            let output_dir = args
                .output
                .unwrap_or_else(|| PathBuf::from("birds_hierarchy"));
            let db_path = output_dir.join(".avicull/manifest.sqlite");
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db
                    .get_latest_run_id()?
                    .ok_or_else(|| anyhow!("No existing run found in manifest.sqlite"))?,
            };

            println!(
                "Relinking run '{}' with min-confidence: {:.2}...",
                run_id, args.min_confidence
            );
            let cached_bundles = db.get_all_bundles(Some(&run_id))?;
            println!("Loaded {} cached bundle predictions.", cached_bundles.len());

            // Clean previous links
            let unlinked = clean_run_hardlinks(&db, &run_id)?;
            println!("Unlinked {} previous hard links.", unlinked);

            let mut new_ops = Vec::new();
            for (mut dec, bundle) in cached_bundles {
                if dec.final_confidence >= args.min_confidence {
                    dec.routing_tier = RoutingTier::ConfirmedSpecies;
                } else if dec.detection_confidence >= 0.25 {
                    dec.routing_tier = RoutingTier::NeedsReview;
                } else {
                    dec.routing_tier = RoutingTier::NoBirdDetected;
                }

                let ops = materialize_bundle_hardlinks(&bundle, &dec, &output_dir, &run_id, false)?;
                new_ops.extend(ops);
            }

            db.record_hardlink_operations(&new_ops)?;
            println!("✓ Re-materialized {} hard links instantly.", new_ops.len());
        }

        Commands::Clean(args) => {
            println!("== avicull clean ==");
            let output_dir = args
                .output
                .unwrap_or_else(|| PathBuf::from("birds_hierarchy"));
            let db_path = output_dir.join(".avicull/manifest.sqlite");
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db
                    .get_latest_run_id()?
                    .ok_or_else(|| anyhow!("No existing run found in manifest.sqlite"))?,
            };

            println!("Cleaning hard links for run '{}'...", run_id);
            let count = clean_run_hardlinks(&db, &run_id)?;
            println!(
                "✓ Unlinked {} files in output directory. Input directory remains untouched.",
                count
            );
        }

        Commands::Report(args) => {
            let output_dir = args
                .output
                .unwrap_or_else(|| PathBuf::from("birds_hierarchy"));
            let db_path = output_dir.join(".avicull/manifest.sqlite");
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => Some(id),
                None => db.get_latest_run_id()?,
            };

            let cached = db.get_all_bundles(run_id.as_deref())?;

            if args.format.to_lowercase() == "csv" {
                println!("bundle_id,trip_folder,original_stem,species,final_conf,routing_tier");
                for (dec, b) in cached {
                    println!(
                        "{},{},{},{},{:.2},{:?}",
                        b.bundle_id,
                        b.trip_folder_name,
                        b.original_stem,
                        dec.assigned_species_slug,
                        dec.final_confidence,
                        dec.routing_tier
                    );
                }
            } else {
                let records: Vec<_> = cached
                    .into_iter()
                    .map(|(dec, b)| {
                        serde_json::json!({
                            "bundle_id": b.bundle_id,
                            "trip_folder": b.trip_folder_name,
                            "original_stem": b.original_stem,
                            "assigned_species_slug": dec.assigned_species_slug,
                            "final_confidence": dec.final_confidence,
                            "routing_tier": format!("{:?}", dec.routing_tier),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&records)?);
            }
        }
    }

    Ok(())
}
