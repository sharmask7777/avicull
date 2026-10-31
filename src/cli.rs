use crate::types::OrganizationMode;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

pub use crate::organizer::chunk::parse_size_budget;

#[derive(Parser, Debug)]
#[command(
    name = "avicull",
    author = "avicull contributors",
    version = env!("CARGO_PKG_VERSION"),
    about = "Local AI Bird Photo Organizer for macOS Apple Silicon",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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

    /// Set or update the geographic location (GPS coordinates or gazetteer place name) for a directory
    Location(LocationArgs),

    /// Display interactive help topics, workflow manuals, and CLI reference
    Help(HelpArgs),

    /// Generate an uncommitted minimal local test dataset from a user photo directory
    Dataset(DatasetArgs),
}

#[derive(Args, Debug)]
pub struct DatasetArgs {
    /// Source photo directory containing user trip photos
    #[arg(short, long)]
    pub source: PathBuf,

    /// Target destination directory (defaults to tests/fixtures/local_user_samples)
    #[arg(short, long)]
    pub target: Option<PathBuf>,

    /// Overwrite existing target directory if present
    #[arg(short, long, default_value_t = false)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct LocationArgs {
    /// Read-only source directory of trip photos (hierarchy mode) or photo root (in-place mode)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to SQLite manifest database (defaults to <OUTPUT>/.avicull/manifest.sqlite)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Directory or trip folder key to assign the location to
    #[arg(short, long)]
    pub dir: Option<String>,

    /// Filename, stem, or bundle ID of a specific photo asset to assign the location to
    #[arg(short, long)]
    pub file: Option<String>,

    /// Location name or GPS coordinates (e.g. "Leh" or "34.1526, 77.5771")
    #[arg(short, long)]
    pub location: String,

    /// Trigger reclassification of affected photos using the updated location priors
    #[arg(short, long, default_value_t = false)]
    pub reclassify: bool,
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Read-only root directory of trip-based bird photos
    #[arg(short, long)]
    pub input: PathBuf,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Optional TOML file mapping trip folder keywords to {lat, lon}
    #[arg(long)]
    pub locations_file: Option<PathBuf>,

    /// Fallback default latitude
    #[arg(long)]
    pub default_lat: Option<f64>,

    /// Fallback default longitude
    #[arg(long)]
    pub default_lon: Option<f64>,
}

#[derive(Args, Debug)]
pub struct OrganizeArgs {
    /// Read-only root directory of trip-based bird photos
    #[arg(short, long)]
    pub input: PathBuf,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Compute classifications and write SQLite plan without creating filesystem hard links
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Force re-running ML detection and classification on all bundles, bypassing SQLite cache
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// Threshold for Confirmed vs _Needs_Review (default: 0.65)
    #[arg(long, default_value_t = 0.65)]
    pub min_confidence: f32,

    /// YOLO11 bird detection confidence floor (default: 0.25)
    #[arg(long, default_value_t = 0.25)]
    pub min_detect: f32,

    /// Max timestamp gap for burst clustering in ms (default: 3000)
    #[arg(long, default_value_t = 3000)]
    pub burst_window_ms: i64,

    /// Filename score: [burst-consensus | per-frame] (default: burst-consensus)
    #[arg(long, default_value = "burst-consensus")]
    pub confidence_mode: String,

    /// Organization mode: [in-place | hierarchy] (default: in-place)
    #[arg(long, value_enum, default_value_t = OrganizationMode::InPlaceFileRenameToDescriptiveNames)]
    pub mode: OrganizationMode,

    /// Strength of SINR geographical prior [0.0..2.0] (default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    pub geo_prior_weight: f32,

    /// Optional TOML file mapping trip folder keywords to {lat, lon}
    #[arg(long)]
    pub locations_file: Option<PathBuf>,

    /// Fallback default latitude
    #[arg(long)]
    pub default_lat: Option<f64>,

    /// Fallback default longitude
    #[arg(long)]
    pub default_lon: Option<f64>,

    /// Directory containing exported .onnx and .safetensors files
    #[arg(long)]
    pub models_dir: Option<PathBuf>,

    /// Rayon worker thread count for I/O and preprocessing
    #[arg(long)]
    pub threads: Option<usize>,

    /// Override location name or coordinates (lat,lon) for photos lacking GPS
    #[arg(short = 'l', long)]
    pub location: Option<String>,

    /// Run non-interactively without prompting
    #[arg(long, default_value_t = false)]
    pub no_prompt: bool,

    /// Maximum cache size budget for chunked processing loop (default: 5GB)
    #[arg(long, default_value = "5GB")]
    pub max_cache_size: String,
}

#[derive(Args, Debug)]
pub struct RelinkArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// New threshold for Confirmed vs _Needs_Review (default: 0.65)
    #[arg(long, default_value_t = 0.65)]
    pub min_confidence: f32,

    /// Specific run_id to relink (defaults to latest run)
    #[arg(long)]
    pub run_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Specific run_id to clean (defaults to latest run)
    #[arg(long)]
    pub run_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct ReportArgs {
    /// Input trip archive root (defaults output to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target root directory containing .avicull/manifest.sqlite (defaults to <input>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to manifest.sqlite database directly
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Specific run_id to report on (defaults to latest run)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output format: json, csv, or table (default: json)
    #[arg(long, default_value = "json")]
    pub format: String,
}

#[derive(Args, Debug)]
pub struct ModelsArgs {
    /// Action: [status | update] (default: status)
    #[arg(default_value = "status")]
    pub action: String,

    /// Target cache directory (defaults to ~/.cache/avicull/models)
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Only check if an update is available without downloading or replacing the binary
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Args, Debug)]
pub struct CorrectArgs {
    /// Read-only source directory of trip photos (hierarchy mode) or photo root (in-place mode)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target root directory (defaults to <INPUT>/birds_hierarchy)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Path to SQLite manifest database (defaults to <OUTPUT>/.avicull/manifest.sqlite)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Filename, stem, or bundle ID of the photo asset to reclassify
    #[arg(short, long)]
    pub file: String,

    /// Correct species name or directory slug (e.g. "Painted Stork" or "Painted_Stork")
    #[arg(short, long)]
    pub species: String,

    /// Organization mode: [in-place | hierarchy]
    #[arg(long, value_enum)]
    pub mode: Option<OrganizationMode>,

    /// Propagate correction to all frames in the same burst
    #[arg(
        short = 'b',
        long = "propagate-burst",
        visible_alias = "burst",
        default_value_t = false
    )]
    pub propagate_burst: bool,
}

#[derive(Args, Debug)]
pub struct HelpArgs {
    /// Topic or command to display: [quickstart | organize | scan | invariants | models | update | all]
    #[arg(value_name = "TOPIC")]
    pub topic: Option<String>,
}
