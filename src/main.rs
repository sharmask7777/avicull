use anyhow::{anyhow, Context, Result};
use avicull::burst::{smooth_burst_consensus, ClassifiedBundleItem};
use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::classifier::{MockSpeciesClassifier, OrtSpeciesClassifier, SpeciesClassifier};
use avicull::db::{ExistingBundleMap, ManifestDb};
use avicull::detector::{
    select_and_pad_bird_crop, BirdDetector, MockBirdDetector, OrtBirdDetector,
};
use avicull::geo::resolve_locations;
use avicull::location::gazetteer::GazetteerEngine;
use avicull::location::path_parser::extract_candidate_location_tokens;
use avicull::location::resolver::resolve_trip_locations;
use avicull::organizer::{
    clean_run_hardlinks, clean_run_operations, execute_inplace_bundle_rename,
    materialize_bundle_hardlinks,
};
use avicull::scanner::{
    extract_embedded_jpeg_preview, scan_input_tree_with_skip, validate_volume_and_disjointness,
};
use avicull::types::{
    ConfidenceMode, GpsCoordinate, LocationSource, OrganizationMode, RoutingTier,
};
use clap::{Args, Parser, Subcommand};
use image::DynamicImage;
use std::fs::File;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "avicull",
    author = "avicull contributors",
    version = env!("CARGO_PKG_VERSION"),
    about = "Local AI Bird Photo Organizer for macOS Apple Silicon",
    disable_help_subcommand = true
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

    /// Inspect status or update local eBird taxonomy and AI models cache (~/.cache/avicull/models)
    Models(ModelsArgs),

    /// Check for newer releases or self-update avicull to the latest prebuilt version
    Update(UpdateArgs),

    /// Override classification for a photo bundle, update SQLite manifest and disk structure
    Correct(CorrectArgs),

    /// Display interactive help topics, workflow manuals, and CLI reference
    Help(HelpArgs),
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

    /// Force re-running ML detection and classification on all bundles, bypassing SQLite cache
    #[arg(long, default_value_t = false)]
    force: bool,

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

    /// Organization mode: [in-place | hierarchy] (default: in-place)
    #[arg(long, value_enum, default_value_t = OrganizationMode::InPlaceFileRenameToDescriptiveNames)]
    mode: OrganizationMode,

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

    /// Override location name or coordinates (lat,lon) for photos lacking GPS
    #[arg(short = 'l', long)]
    location: Option<String>,

    /// Run non-interactively without prompting
    #[arg(long, default_value_t = false)]
    no_prompt: bool,
}

#[derive(Args, Debug)]
struct RelinkArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    db: Option<PathBuf>,

    /// New threshold for Confirmed vs _Needs_Review (default: 0.65)
    #[arg(long, default_value_t = 0.65)]
    min_confidence: f32,

    /// Specific run_id to relink (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Args, Debug)]
struct CleanArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    db: Option<PathBuf>,

    /// Specific run_id to clean (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,
}

#[derive(Args, Debug)]
struct ReportArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    db: Option<PathBuf>,

    /// Specific run_id to report on (defaults to latest run)
    #[arg(long)]
    run_id: Option<String>,

    /// Output format: json, csv, or table (default: json)
    #[arg(long, default_value = "json")]
    format: String,
}

#[derive(Args, Debug)]
struct ModelsArgs {
    /// Action: [status | update] (default: status)
    #[arg(default_value = "status")]
    action: String,

    /// Target cache directory (defaults to ~/.cache/avicull/models)
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct UpdateArgs {
    /// Only check if an update is available without downloading or replacing the binary
    #[arg(long, default_value_t = false)]
    check: bool,
}

#[derive(Args, Debug)]
struct CorrectArgs {
    /// Read-only source directory of trip photos (hierarchy mode) or photo root (in-place mode)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to SQLite manifest database (defaults to <OUTPUT>/.avicull/manifest.sqlite)
    #[arg(long)]
    db: Option<PathBuf>,

    /// Filename, stem, or bundle ID of the photo asset to reclassify
    #[arg(short, long)]
    file: String,

    /// Correct species name or directory slug (e.g. "Painted Stork" or "Painted_Stork")
    #[arg(short, long)]
    species: String,

    /// Organization mode: [in-place | hierarchy]
    #[arg(long, value_enum)]
    mode: Option<OrganizationMode>,
}

#[derive(Args, Debug)]
struct HelpArgs {
    /// Topic or command to display: [quickstart | organize | scan | invariants | models | update | all]
    #[arg(value_name = "TOPIC")]
    topic: Option<String>,
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

fn prepare_image_for_inference(
    bundle: &avicull::types::AssetBundle,
    temp_dir: &Path,
    idx: usize,
) -> Option<PathBuf> {
    for comp in &bundle.companion_images {
        let ext = comp
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            return Some(comp.clone());
        }
    }

    let prim_ext = bundle
        .primary_image
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if prim_ext == "jpg" || prim_ext == "jpeg" || prim_ext == "png" {
        return Some(bundle.primary_image.clone());
    }

    // Try extracting embedded preview for RAW
    if let Ok(mut f) = File::open(&bundle.primary_image) {
        let mut buffer = Vec::new();
        if f.read_to_end(&mut buffer).is_ok() {
            if let Some(preview_bytes) = extract_embedded_jpeg_preview(&buffer) {
                let out_path = temp_dir.join(format!("preview_{}.jpg", idx));
                if std::fs::write(&out_path, preview_bytes).is_ok() {
                    return Some(out_path);
                }
            }
        }
    }

    Some(bundle.primary_image.clone())
}

fn run_batch_inference(
    bundles: &[&avicull::types::AssetBundle],
    models_dir: &Path,
    min_detect: f32,
    geo_prior_weight: f32,
    bundle_regions: &std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, ClassifiedBundleItem>> {
    let infer_script = Path::new("scripts/infer.py");
    let embeddings_file = models_dir.join("species_embeddings.safetensors");

    if !infer_script.exists() || !embeddings_file.exists() {
        return Err(anyhow!("Real models or infer.py script not found"));
    }

    let temp_dir = tempfile::tempdir()?;
    let batch_file = temp_dir.path().join("batch_tasks.json");
    let output_file = temp_dir.path().join("batch_results.json");

    #[derive(serde::Serialize)]
    struct BatchTask {
        bundle_id: String,
        primary_image: PathBuf,
        trip_folder_name: String,
        latitude: Option<f64>,
        longitude: Option<f64>,
        region_code: String,
    }

    let mut tasks = Vec::with_capacity(bundles.len());

    for (idx, bundle) in bundles.iter().enumerate() {
        let img_path = prepare_image_for_inference(bundle, temp_dir.path(), idx)
            .unwrap_or_else(|| bundle.primary_image.clone());
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

    let output = std::process::Command::new("uv")
        .args([
            "run",
            "--with",
            "torch,open-clip-torch,safetensors,ultralytics,pillow",
            "python3",
            "scripts/infer.py",
            "--models-dir",
            models_dir.to_str().unwrap_or("models"),
            "--batch-file",
            batch_file.to_str().unwrap(),
            "--output-file",
            output_file.to_str().unwrap(),
            "--min-detect",
            &min_detect.to_string(),
            "--geo-prior-weight",
            &geo_prior_weight.to_string(),
        ])
        .output()?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Python inference runner failed: {}", err_msg));
    }

    #[derive(serde::Deserialize)]
    struct InferResultItem {
        bundle_id: String,
        detection_confidence: f32,
        top_predictions: Vec<avicull::types::SpeciesPrediction>,
        embedding: Vec<f32>,
    }

    let results_data = std::fs::read(&output_file)?;
    let parsed: Vec<InferResultItem> = serde_json::from_slice(&results_data)?;

    let mut map = std::collections::HashMap::new();
    let bundle_map: std::collections::HashMap<_, _> =
        bundles.iter().map(|b| (&b.bundle_id, *b)).collect();

    for item in parsed {
        if let Some(bundle) = bundle_map.get(&item.bundle_id) {
            let posteriors: Vec<(String, f32)> = item
                .top_predictions
                .iter()
                .map(|p| (p.taxon_id.clone(), p.combined_score))
                .collect();

            map.insert(
                item.bundle_id.clone(),
                ClassifiedBundleItem {
                    bundle_id: item.bundle_id,
                    trip_folder: bundle.trip_folder_name.clone(),
                    capture_timestamp_ms: bundle.capture_timestamp_ms,
                    detection_confidence: item.detection_confidence,
                    bbox_norm: None,
                    embedding: item.embedding,
                    top_predictions: item.top_predictions,
                    posteriors,
                },
            );
        }
    }

    Ok(map)
}

fn run_fallback_parallel_classification(
    new_bundles: &[&avicull::types::AssetBundle],
    detector: &std::sync::Arc<Box<dyn BirdDetector>>,
    classifier: &std::sync::Arc<Box<dyn SpeciesClassifier>>,
    min_detect: f32,
    geo_prior_weight: f32,
    bundle_regions: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, ClassifiedBundleItem> {
    use rayon::prelude::*;
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
                let region = bundle_regions.get(&bundle.bundle_id).map(|s| s.as_str());
                let (emb, preds) = classifier
                    .classify_with_region(&cropped, bundle.gps_coords, region, geo_prior_weight)
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
        .collect()
}

fn resolve_or_prompt_models_dir(
    requested: Option<PathBuf>,
    no_prompt: bool,
) -> Result<Option<PathBuf>> {
    if let Some(r) = requested {
        return Ok(Some(r));
    }

    // Check local ./models directory
    let local_models = PathBuf::from("models");
    if local_models.join("species_embeddings.safetensors").exists() {
        return Ok(Some(local_models));
    }

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
            return Ok(Some(c_dir.clone()));
        }
    }

    // Avoid prompting in non-interactive environments, headless test runners, or when --no-prompt is specified
    use std::io::IsTerminal;
    if no_prompt || !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    // Prompt user in interactive terminal sessions
    if let Some(ref c_dir) = cache_dir {
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
                let status_res = std::process::Command::new("uv")
                    .args([
                        "run",
                        "--with",
                        "torch,open-clip-torch,safetensors,ultralytics",
                        "python3",
                        "scripts/build_taxonomy_embeddings.py",
                        c_dir.to_str().unwrap(),
                    ])
                    .status();

                match status_res {
                    Ok(status) if status.success() => {
                        println!(
                            "[avicull] ✓ Models successfully installed to '{}' and ready for reuse across all future runs.",
                            c_dir.display()
                        );
                        return Ok(Some(c_dir.clone()));
                    }
                    _ => {
                        eprintln!(
                            "[avicull] Warning: Failed to download models. Falling back to mock classifier."
                        );
                    }
                }
            } else {
                println!("[avicull] Skipping model download. Falling back to mock classifier.");
            }
        }
    }

    Ok(None)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Perform periodic background update check on normal CLI invocations
    if !matches!(cli.command, Commands::Update(_)) {
        if let Some(notice) = avicull::update::check_for_update_notice(env!("CARGO_PKG_VERSION")) {
            eprintln!("{}\n", notice);
        }
    }

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

            let is_inplace = matches!(
                args.mode,
                OrganizationMode::InPlaceFileRenameToDescriptiveNames
            );

            let (operating_root, db_path, skip_dir) = if is_inplace {
                if args.output.is_some() {
                    println!(
                        "[avicull] Note: In in-place rename mode, photos are organized directly in '--input'; '--output' is ignored."
                    );
                }
                let db_p = args.input.join(".avicull/manifest.sqlite");
                (args.input.clone(), db_p, None)
            } else {
                let output_dir = args
                    .output
                    .unwrap_or_else(|| args.input.join("birds_hierarchy"));
                validate_volume_and_disjointness(&args.input, &output_dir)?;
                let db_p = output_dir.join(".avicull/manifest.sqlite");
                (output_dir.clone(), db_p, Some(output_dir))
            };

            let db = ManifestDb::open_or_create(&db_path)?;
            let mut existing_bundle_map = if args.force {
                ExistingBundleMap::new()
            } else {
                db.get_existing_bundle_map().unwrap_or_default()
            };

            let mut all_discovered = scan_input_tree_with_skip(&args.input, skip_dir.as_deref())?;
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

            // Location resolution via Gazetteer and Path Inference for bundles lacking EXIF GPS
            let mut bundle_regions: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();

            // Group trips where bundles lack EXIF GPS, or all trips if global --location is specified
            let mut trip_map: std::collections::BTreeMap<
                String,
                Vec<&avicull::types::AssetBundle>,
            > = std::collections::BTreeMap::new();
            for bundle in &all_discovered {
                let lacks_exif = bundle
                    .gps_coords
                    .map(|c| c.source != LocationSource::ExifGps)
                    .unwrap_or(true);
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

                let engine = GazetteerEngine::load_embedded()?;
                let mut stdin = std::io::stdin().lock();
                let mut stdout = std::io::stdout();
                let is_interactive = !args.no_prompt && std::io::stdin().is_terminal();

                let resolved = resolve_trip_locations(
                    &trips_to_tokens,
                    &engine,
                    &db,
                    &mut stdin,
                    &mut stdout,
                    is_interactive,
                    args.location.as_deref(),
                )?;

                for bundle in &mut all_discovered {
                    let has_exif_gps = bundle
                        .gps_coords
                        .map(|c| c.source == LocationSource::ExifGps)
                        .unwrap_or(false);

                    if let Some(loc) = resolved.get(&bundle.trip_folder_name) {
                        bundle.resolved_location_name = Some(loc.name.clone());
                        if !has_exif_gps {
                            bundle.gps_coords = Some(GpsCoordinate {
                                latitude: loc.latitude,
                                longitude: loc.longitude,
                                source: LocationSource::TripFolderGazetteer,
                            });
                        }
                        if !loc.region_code.is_empty() {
                            bundle_regions
                                .insert(bundle.bundle_id.clone(), loc.region_code.clone());
                        }
                    } else if let Ok(Some(cached_loc)) =
                        db.get_directory_location(&bundle.trip_folder_name)
                    {
                        bundle.resolved_location_name = Some(cached_loc.name.clone());
                        if !has_exif_gps && bundle.gps_coords.is_none() {
                            bundle.gps_coords = Some(GpsCoordinate {
                                latitude: cached_loc.latitude,
                                longitude: cached_loc.longitude,
                                source: LocationSource::TripFolderGazetteer,
                            });
                        }
                        if !cached_loc.region_code.is_empty() {
                            bundle_regions.insert(bundle.bundle_id.clone(), cached_loc.region_code);
                        }
                    }
                }
            }

            // Ensure any bundles that already have cached directory location in db have their region_code and location attached
            for bundle in &mut all_discovered {
                if bundle.resolved_location_name.is_none() {
                    if let Ok(Some(loc)) = db.get_directory_location(&bundle.trip_folder_name) {
                        bundle.resolved_location_name = Some(loc.name);
                    }
                }
                if !bundle_regions.contains_key(&bundle.bundle_id) {
                    if let Ok(Some(loc)) = db.get_directory_location(&bundle.trip_folder_name) {
                        if !loc.region_code.is_empty() {
                            bundle_regions.insert(bundle.bundle_id.clone(), loc.region_code);
                        }
                    }
                }
            }

            // Reconcile any manual filesystem moves in the output directory (hierarchy mode)
            let mut reconciled_moves_count = 0;
            if args.mode == OrganizationMode::HierarchicalHardlinkHierarchy
                && operating_root.exists()
                && !existing_bundle_map.is_empty()
            {
                match avicull::organizer::reconcile_manual_disk_moves(
                    &operating_root,
                    &db,
                    &mut existing_bundle_map,
                ) {
                    Ok(count) => {
                        reconciled_moves_count = count;
                        if count > 0 {
                            println!(
                                "Reconciliation: Reconciled {} manual filesystem move(s) into durable SQLite overrides.",
                                count
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to reconcile manual filesystem moves: {}",
                            e
                        );
                    }
                }
            }

            // Partition into new vs cached bundles using bundle_id, source_inode, relative_path, or portable key
            let mut new_bundles = Vec::new();
            let mut cached_items = std::collections::HashMap::new();

            for bundle in &all_discovered {
                if let Some((existing_decision, _)) = existing_bundle_map.get_by_bundle(bundle) {
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
                let catalog_path = operating_root.join("catalog.html");
                if !catalog_path.exists() || reconciled_moves_count > 0 {
                    let trip_name = args
                        .input
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Avian Photos");
                    println!(
                        "Generating standalone offline HTML photo catalog at '{}'...",
                        catalog_path.display()
                    );
                    let mut catalog_items = Vec::with_capacity(all_discovered.len());
                    for bundle in &all_discovered {
                        if let Some((existing_decision, _)) =
                            existing_bundle_map.get_by_bundle(bundle)
                        {
                            let cat_item = CatalogItem::from_bundle_and_decision(
                                bundle,
                                existing_decision,
                                Some(bundle.primary_image.clone()),
                                Some(bundle.companion_images.clone()),
                                Some(bundle.sidecar_files.clone()),
                            );
                            catalog_items.push(cat_item);
                        }
                    }
                    generate_html_catalog(trip_name, &catalog_items, args.mode, &catalog_path)?;
                }

                println!(
                    "✓ All {} photo bundles are already up to date in '{}'.",
                    all_discovered.len(),
                    operating_root.display()
                );
                return Ok(());
            }

            let conf_mode: ConfidenceMode =
                args.confidence_mode.parse().map_err(|e| anyhow!("{}", e))?;

            let models_dir = resolve_or_prompt_models_dir(args.models_dir, args.no_prompt)?;

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

            use std::sync::Arc;
            let detector = Arc::new(detector);
            let classifier = Arc::new(classifier);

            let new_classified = if let Some(ref m_dir) = models_dir {
                if m_dir.join("species_embeddings.safetensors").exists()
                    && Path::new("scripts/infer.py").exists()
                {
                    println!(
                        "Running real AI bird detection (YOLO11) and classification (BioCLIP) on {} new bundles...",
                        new_bundles.len()
                    );
                    match run_batch_inference(
                        &new_bundles,
                        m_dir,
                        args.min_detect,
                        args.geo_prior_weight,
                        &bundle_regions,
                    ) {
                        Ok(res) => res,
                        Err(e) => {
                            eprintln!(
                                "Warning: Real inference error ({}), using fallback: {}",
                                e, e
                            );
                            run_fallback_parallel_classification(
                                &new_bundles,
                                &detector,
                                &classifier,
                                args.min_detect,
                                args.geo_prior_weight,
                                &bundle_regions,
                            )
                        }
                    }
                } else {
                    println!(
                        "Running detection and classification on {} new bundles in parallel...",
                        new_bundles.len()
                    );
                    run_fallback_parallel_classification(
                        &new_bundles,
                        &detector,
                        &classifier,
                        args.min_detect,
                        args.geo_prior_weight,
                        &bundle_regions,
                    )
                }
            } else {
                println!(
                    "Running detection and classification on {} new bundles in parallel...",
                    new_bundles.len()
                );
                run_fallback_parallel_classification(
                    &new_bundles,
                    &detector,
                    &classifier,
                    args.min_detect,
                    args.geo_prior_weight,
                    &bundle_regions,
                )
            };

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
            let mut decisions = smooth_burst_consensus(
                &all_classified_items,
                args.burst_window_ms,
                0.80,
                conf_mode,
                args.min_confidence,
            );

            // Preserve user manual overrides across burst consensus and re-runs
            for (bundle, decision) in all_discovered.iter().zip(decisions.iter_mut()) {
                if let Some((existing_dec, _)) = existing_bundle_map.get_by_bundle(bundle) {
                    if existing_dec.is_user_override {
                        decision.assigned_species_slug = existing_dec.assigned_species_slug.clone();
                        decision.routing_tier = existing_dec.routing_tier;
                        decision.final_confidence = existing_dec.final_confidence;
                        decision.is_user_override = true;
                    }
                }
            }

            let run_id = format!("run-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
            let mode_str = match args.mode {
                OrganizationMode::InPlaceFileRenameToDescriptiveNames => "in_place",
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
            db.record_bundles(&run_id, &decisions, &all_discovered)?;

            let mut catalog_items = Vec::with_capacity(all_discovered.len());
            let mut total_ops_count = 0usize;

            if is_inplace {
                println!(
                    "Executing in-place file renames (dry_run: {})...",
                    args.dry_run
                );
                for (bundle, decision) in all_discovered.iter().zip(decisions.iter()) {
                    let renames = if decision.routing_tier == RoutingTier::NoBirdDetected {
                        // Bundles where no bird is detected are preserved in-place without renaming.
                        Vec::new()
                    } else if !args.dry_run {
                        let species_name = match decision.routing_tier {
                            RoutingTier::ConfirmedSpecies => {
                                if let Some(top) = decision.top_predictions.first() {
                                    top.common_name.clone()
                                } else {
                                    decision.assigned_species_slug.clone()
                                }
                            }
                            RoutingTier::NeedsReview => {
                                if let Some(top) = decision.top_predictions.first() {
                                    format!("Needs_Review_{}", top.common_name)
                                } else {
                                    format!("Needs_Review_{}", decision.assigned_species_slug)
                                }
                            }
                            RoutingTier::NoBirdDetected => unreachable!(),
                        };
                        execute_inplace_bundle_rename(bundle, &species_name, None, None)?
                    } else {
                        Vec::new()
                    };

                    for (orig, renamed) in &renames {
                        db.record_inplace_rename(&run_id, orig, renamed)?;
                    }
                    total_ops_count += renames.len();

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
            } else {
                println!("Materializing hard links (dry_run: {})...", args.dry_run);
                let mut total_ops = Vec::new();
                for (bundle, decision) in all_discovered.iter().zip(decisions.iter()) {
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
                }
                total_ops_count = total_ops.len();
                db.record_hardlink_operations(&total_ops)?;
            }

            db.complete_run(&run_id)?;

            let catalog_path = operating_root.join("catalog.html");
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
                "✓ Done! Confirmed: {}, Needs Review: {}, No Bird: {}. Operations: {}",
                confirmed, review, no_bird, total_ops_count
            );
        }

        Commands::Relink(args) => {
            println!("== avicull relink ==");
            let output_dir = args
                .output
                .clone()
                .or_else(|| args.input.as_ref().map(|p| p.join("birds_hierarchy")))
                .unwrap_or_else(|| PathBuf::from("birds_hierarchy"));
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if args.output.is_some() {
                output_dir.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                output_dir.join(".avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db.get_latest_run_id()?.ok_or_else(|| {
                    anyhow!(
                        "No existing run found in manifest.sqlite at '{}'",
                        db_path.display()
                    )
                })?,
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

                db.update_bundle_routing_tier(&bundle.bundle_id, dec.routing_tier)?;
                let ops = materialize_bundle_hardlinks(&bundle, &dec, &output_dir, &run_id, false)?;
                new_ops.extend(ops);
            }

            db.record_hardlink_operations(&new_ops)?;
            println!("✓ Re-materialized {} hard links instantly.", new_ops.len());
        }

        Commands::Clean(args) => {
            println!("== avicull clean ==");
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if let Some(ref out) = args.output {
                out.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db.get_latest_run_id()?.ok_or_else(|| {
                    anyhow!(
                        "No existing run found in manifest.sqlite at '{}'",
                        db_path.display()
                    )
                })?,
            };

            println!("Cleaning operations for run '{}'...", run_id);
            let count = clean_run_operations(&db, &run_id)?;
            println!("✓ Cleaned {} operations for run '{}'.", count, run_id);
        }

        Commands::Report(args) => {
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if let Some(ref out) = args.output {
                out.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => Some(id),
                None => db.get_latest_run_id()?,
            };

            let cached = db.get_all_bundles(run_id.as_deref())?;

            let fmt = args.format.to_lowercase();
            if fmt == "csv" {
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
            } else if fmt == "table" {
                use std::collections::BTreeMap;
                let mut species_stats: BTreeMap<String, (usize, f32)> = BTreeMap::new();
                for (dec, _) in &cached {
                    let entry = species_stats
                        .entry(dec.assigned_species_slug.clone())
                        .or_insert((0, 0.0));
                    entry.0 += 1;
                    entry.1 += dec.final_confidence;
                }
                println!(
                    "{:<35} | {:<8} | {:<10}",
                    "Species Slug", "Count", "Mean Conf"
                );
                println!("{:-<35}-+-{:-<8}-+-{:-<10}", "", "", "");
                for (species, (count, total_conf)) in species_stats {
                    let mean_conf = if count > 0 {
                        total_conf / count as f32
                    } else {
                        0.0
                    };
                    println!("{:<35} | {:<8} | {:<10.2}", species, count, mean_conf);
                }
                println!("\nTotal Bundles: {}", cached.len());
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

        Commands::Models(args) => {
            let home = std::env::var("HOME").ok();
            let default_cache = home.map(|h| PathBuf::from(h).join(".cache/avicull/models"));
            let cache_dir = args
                .cache_dir
                .or(default_cache)
                .unwrap_or_else(|| PathBuf::from("models"));

            println!("== avicull models ==");
            println!("Avian Model & Taxonomy Cache: '{}'", cache_dir.display());

            let tax_path = cache_dir.join("taxonomy.json");
            let emb_path = cache_dir.join("species_embeddings.safetensors");
            let manifest_path = cache_dir.join("manifest.json");

            if args.action == "update" {
                println!(
                    "Fetching latest Cornell eBird global taxonomy and recomputing embeddings..."
                );
                std::fs::create_dir_all(&cache_dir)?;
                let status = std::process::Command::new("uv")
                    .args([
                        "run",
                        "--with",
                        "torch,open-clip-torch,safetensors,ultralytics",
                        "python3",
                        "scripts/build_taxonomy_embeddings.py",
                        cache_dir.to_str().unwrap(),
                    ])
                    .status()?;
                if status.success() {
                    println!(
                        "✓ Successfully updated model and taxonomy cache in '{}'",
                        cache_dir.display()
                    );
                } else {
                    return Err(anyhow!("Failed to update models"));
                }
            } else {
                let tax_status = if tax_path.exists() {
                    let sz = std::fs::metadata(&tax_path).map(|m| m.len()).unwrap_or(0);
                    format!("Present ({:.2} MB)", sz as f64 / 1_048_576.0)
                } else {
                    "Missing (run 'avicull models update' to install)".to_string()
                };

                let emb_status = if emb_path.exists() {
                    let sz = std::fs::metadata(&emb_path).map(|m| m.len()).unwrap_or(0);
                    format!("Present ({:.2} MB)", sz as f64 / 1_048_576.0)
                } else {
                    "Missing (run 'avicull models update' to install)".to_string()
                };

                println!("  - Taxonomy (taxonomy.json):             {}", tax_status);
                println!(
                    "  - Embeddings (species_embeddings.safetensors): {}",
                    emb_status
                );

                if let Ok(meta) = std::fs::metadata(&manifest_path) {
                    if let Ok(mod_time) = meta.modified() {
                        if let Ok(dur) = mod_time.elapsed() {
                            let days = dur.as_secs() / 86400;
                            println!(
                                "  - Last Updated:                         {} day(s) ago",
                                days
                            );
                        }
                    }
                }
            }
        }

        Commands::Update(args) => {
            avicull::update::perform_self_update(args.check)?;
        }

        Commands::Correct(args) => {
            execute_correct(args)?;
        }

        Commands::Help(args) => {
            let topic = match args.topic.as_deref() {
                Some(t) => Some(t.parse::<avicull::help::HelpTopic>()?),
                None => None,
            };
            print!("{}", avicull::help::render_help(topic));
        }
    }

    Ok(())
}

fn execute_correct(args: CorrectArgs) -> Result<()> {
    println!("== avicull correct ==");

    let db_path = if let Some(ref db_p) = args.db {
        db_p.clone()
    } else if let Some(ref out) = args.output {
        out.join(".avicull/manifest.sqlite")
    } else if let Some(ref inp) = args.input {
        let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
        let inplace_db = inp.join(".avicull/manifest.sqlite");
        if hier_db.exists() {
            hier_db
        } else if inplace_db.exists() {
            inplace_db
        } else {
            hier_db
        }
    } else {
        let hier_db = PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite");
        let inplace_db = PathBuf::from(".avicull/manifest.sqlite");
        if hier_db.exists() {
            hier_db
        } else if inplace_db.exists() {
            inplace_db
        } else {
            hier_db
        }
    };

    if !db_path.exists() {
        return Err(anyhow!(
            "Manifest database not found at '{}'. Run 'avicull organize' first.",
            db_path.display()
        ));
    }

    let db = ManifestDb::open_or_create(&db_path)?;

    let mode = if let Some(m) = args.mode {
        m
    } else if let Ok(Some(m)) = db.get_latest_run_mode() {
        m
    } else if args.output.is_some() || db_path.to_string_lossy().contains("birds_hierarchy") {
        OrganizationMode::HierarchicalHardlinkHierarchy
    } else {
        OrganizationMode::InPlaceFileRenameToDescriptiveNames
    };

    let operating_root = match mode {
        OrganizationMode::HierarchicalHardlinkHierarchy => args
            .output
            .clone()
            .or_else(|| args.input.as_ref().map(|p| p.join("birds_hierarchy")))
            .or_else(|| {
                db_path
                    .parent()
                    .and_then(|p| p.parent())
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
            })
            .unwrap_or_else(|| {
                if Path::new("birds_hierarchy").exists() {
                    PathBuf::from("birds_hierarchy")
                } else {
                    PathBuf::from(".")
                }
            }),
        OrganizationMode::InPlaceFileRenameToDescriptiveNames => args
            .input
            .clone()
            .or_else(|| {
                db_path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
            })
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    let (updated_dec, bundle) = avicull::organizer::apply_bundle_correction(
        &db,
        &operating_root,
        mode,
        &args.file,
        &args.species,
        args.input.as_deref(),
    )?;

    println!(
        "✓ Reclassified '{}' to '{}' (routing_tier: {:?}, is_user_override: 1).",
        bundle.original_stem, updated_dec.assigned_species_slug, updated_dec.routing_tier
    );

    Ok(())
}
