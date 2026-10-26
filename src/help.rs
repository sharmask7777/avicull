use anyhow::{anyhow, Error, Result};
use std::fmt;
use std::str::FromStr;

/// Available interactive and reference help topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Quickstart,
    Organize,
    Scan,
    Invariants,
    Models,
    Update,
    All,
}

impl FromStr for HelpTopic {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "quickstart" | "start" => Ok(HelpTopic::Quickstart),
            "organize" | "sort" => Ok(HelpTopic::Organize),
            "scan" | "check" => Ok(HelpTopic::Scan),
            "invariants" | "safety" | "apfs" => Ok(HelpTopic::Invariants),
            "models" | "weights" | "ai" => Ok(HelpTopic::Models),
            "update" | "upgrade" => Ok(HelpTopic::Update),
            "all" | "full" | "manual" => Ok(HelpTopic::All),
            other => Err(anyhow!(
                "Unknown help topic '{}'. Available topics: quickstart, organize, scan, invariants, models, update, all",
                other
            )),
        }
    }
}

impl fmt::Display for HelpTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HelpTopic::Quickstart => write!(f, "quickstart"),
            HelpTopic::Organize => write!(f, "organize"),
            HelpTopic::Scan => write!(f, "scan"),
            HelpTopic::Invariants => write!(f, "invariants"),
            HelpTopic::Models => write!(f, "models"),
            HelpTopic::Update => write!(f, "update"),
            HelpTopic::All => write!(f, "all"),
        }
    }
}

impl HelpTopic {
    /// Returns human-readable topic title.
    pub fn title(&self) -> &'static str {
        match self {
            HelpTopic::Quickstart => "Quickstart & Basic Usage",
            HelpTopic::Organize => "The Organize Pipeline (Detection, Classification & Routing)",
            HelpTopic::Scan => "Pre-Flight Archive Scanning & Location Resolution",
            HelpTopic::Invariants => "Filesystem Safety & APFS Hard Link Invariants",
            HelpTopic::Models => "Neural Models & eBird Taxonomy Cache",
            HelpTopic::Update => "Self-Update & Release Management",
            HelpTopic::All => "Comprehensive avicull User Manual",
        }
    }

    /// Returns detailed help content for the topic.
    pub fn content(&self) -> &'static str {
        match self {
            HelpTopic::Quickstart => {
                "QUICKSTART & BASIC USAGE\n\
                 ========================\n\n\
                 1. Dry-Run Check:\n\
                    Preview the classification and renaming plan without writing filesystem changes:\n\
                    avicull organize --input /Volumes/Photos/Trips/Leh --dry-run\n\n\
                 2. In-Place Organize & HTML Catalog (Default):\n\
                    Renames photos in place with species, date, and location tags, and creates 'bird_catalog.html':\n\
                    avicull organize --input /Volumes/Photos/Trips/Leh\n\n\
                 3. Hierarchy Mode (Zero-Copy APFS Hard Links):\n\
                    Organize photos into '<input>/birds_hierarchy' using zero-byte hard links:\n\
                    avicull organize --input /Volumes/Photos/Trips/Leh --mode hierarchy\n\n\
                 4. Pre-Flight Scan Only:\n\
                    Verify volume APFS hardlink compatibility and location resolution:\n\
                    avicull scan --input /Volumes/Photos/Trips/Leh\n\n\
                 5. Specifying a Region/Location Override:\n\
                    Help the gazetteer and eBird regional checklists when photos lack GPS:\n\
                    avicull organize --input /Volumes/Photos/Trips/2026-Leh --location \"Leh, Ladakh\"\n\n\
                 6. Safe Rollback Clean:\n\
                    Losslessly revert in-place renames or unlink generated hard links:\n\
                    avicull clean --input /Volumes/Photos/Trips/Leh\n\n\
                 7. Inspect Manifest Database:\n\
                    View classification manifest statistics and burst clustering records:\n\
                    avicull report --db /Volumes/Photos/Trips/Leh/.avicull/manifest.sqlite\n"
            }
            HelpTopic::Organize => {
                "THE ORGANIZE PIPELINE\n\
                 =====================\n\n\
                 `avicull organize` executes a multi-tier pipeline:\n\n\
                 1. Discovery & Sidecar Bundling:\n\
                    Scans '--input' recursively, grouping RAW images (.CR2, .ARW, .NEF) with\n\
                    their associated sidecars (.xmp, .dop) and companion JPEGs into atomic bundles.\n\n\
                 2. Location & Prior Resolution:\n\
                    - Extracts hardware GPS from EXIF tags.\n\
                    - If GPS is absent, extracts location tokens from directory paths and queries\n\
                      the embedded offline gazetteer to resolve geographic coordinates.\n\
                    - Applies eBird regional species checklists as Bayesian priors.\n\n\
                 3. YOLO11 Bird Detection & BioCLIP Classification:\n\
                    - Cropped bird detections are scored against 11,167 eBird species.\n\
                    - Bayesian visual + regional posterior combines visual confidence with regional priors.\n\n\
                 4. Burst Clustering & Consensus Smoothing:\n\
                    Images captured within 3 seconds of each other are clustered into bursts;\n\
                    the consensus winner smooths frame-to-frame variations across burst sequences.\n\n\
                 5. Output Materialization Modes:\n\
                    - In-Place Mode (default):\n\
                      Renames primary image and companion sidecars in place within '--input' to:\n\
                      <OriginalStem>_<Species_Common_Name>[_<Date>][_<Location>].<ext>\n\
                      All renames are tracked in SQLite for lossless rollback.\n\
                    - Hierarchy Mode (--mode hierarchy):\n\
                      Zero-byte POSIX hard links are written to:\n\
                      * Confirmed Species:   <output>/<Species_Common_Name>/\n\
                      * Needs Review:        <output>/_Needs_Review/<Species_Common_Name>/\n\
                      * No Bird Detected:    <output>/_No_Bird_Detected/\n\n\
                 6. Standalone Offline HTML Photo Catalog (bird_catalog.html):\n\
                    Both modes automatically generate 'bird_catalog.html' in the output directory:\n\
                    - Fast gallery with responsive grid and embedded RAW preview thumbnails.\n\
                    - Dynamic search and filter by species, date, location, and confidence tiers.\n\
                    - High-res lightbox modal with EXIF details and ML predictions.\n\
                    - In-browser mislabel correction modal with eBird taxonomy search and\n\
                      one-click generation of 'apply_corrections.sh' batch shell scripts.\n"
            }
            HelpTopic::Scan => {
                "PRE-FLIGHT ARCHIVE SCANNING & LOCATION RESOLUTION\n\
                 =================================================\n\n\
                 `avicull scan` audits your photo collection before running machine learning:\n\n\
                 1. Volume & Filesystem Check:\n\
                    Verifies that the target output directory is on the same APFS volume as input,\n\
                    guaranteeing zero-byte hardlink support.\n\n\
                 2. Bundle Discovery:\n\
                    Groups primary RAW files, sidecars (.xmp), and companion JPEGs into atomic units.\n\n\
                 3. Geolocation & EXIF Audit:\n\
                    Reports which photos possess hardware GPS tags and extracts directory tokens\n\
                    resolved by the offline gazetteer to eBird regional codes.\n"
            }
            HelpTopic::Invariants => {
                "FILESYSTEM SAFETY & APFS HARD LINK INVARIANTS\n\
                 =============================================\n\n\
                 avicull adheres to strict engineering invariants:\n\n\
                 1. Strict Source Immutability:\n\
                    The source directory ('--input') is strictly READ-ONLY. avicull never moves,\n\
                    renames, edits, or deletes any file inside the source tree.\n\n\
                 2. Zero-Byte APFS Hard Links ('std::fs::hard_link'):\n\
                    Output files reference identical underlying inodes (src_ino == dst_ino).\n\
                    Organizing an entire 100 GB photography trip consumes 0 additional disk payload.\n\n\
                 3. Upfront Device & Volume Validation:\n\
                    Hard links cannot cross filesystem boundaries (POSIX EXDEV error).\n\
                    avicull verifies upfront via 'st_dev' that input and output reside on the same\n\
                    volume before processing.\n\n\
                 4. Incremental Updates & Manifest SQLite:\n\
                    Subsequent runs query SQLite to skip already-processed bundles.\n\
                    Only newly added or modified photo bundles undergo neural network inference.\n\n\
                 5. The exFAT Warning:\n\
                    exFAT drives do NOT support hard links. If copying to external drives, format\n\
                    as APFS (for Mac) or NTFS (using 'rsync -aH' to preserve links on Windows).\n"
            }
            HelpTopic::Models => {
                "NEURAL MODELS & EBIRD TAXONOMY CACHE\n\
                 ====================================\n\n\
                 avicull utilizes cutting-edge avian computer vision:\n\n\
                 - Bird Detector:        YOLO11 ONNX optimized for birds\n\
                 - Species Classifier:   BioCLIP Vision Transformer embeddings (512-dim)\n\
                 - Taxonomy Reference:   Cornell eBird / Clements Taxonomy (11,167 species)\n\
                 - Regional Priors:      eBird regional checklists (IN-KA, IN-LA, US-FL, etc.)\n\n\
                 Model Weights Location:\n\
                 - System cache: ~/.cache/avicull/models/\n\
                 - Local override: ./models/ or pass '--models-dir <PATH>'\n\n\
                 On first run, avicull offers to download and initialize the models automatically\n\
                 using 'uv' and Apple Silicon GPU acceleration.\n"
            }
            HelpTopic::Update => {
                "SELF-UPDATE & RELEASE MANAGEMENT\n\
                 ================================\n\n\
                 avicull includes built-in update management:\n\n\
                 1. Check for updates without installing:\n\
                    avicull update --check\n\n\
                 2. Upgrade to the latest release:\n\
                    avicull update\n\n\
                    (Note: If installed in a system directory like /usr/local/bin, run with sudo:\n\
                     sudo avicull update)\n\n\
                 3. Background Update Checks:\n\
                    avicull checks GitHub Releases once every 24 hours in the background.\n\
                    If a newer version is available, a friendly reminder is shown.\n\
                    To disable update checks, set: export AVICULL_NO_UPDATE_CHECK=1\n\n\
                 4. Manual Download:\n\
                    curl -sL https://github.com/sharmask7777/avicull/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz | tar -xz\n"
            }
            HelpTopic::All => {
                "=======================================================================\n\
                 AVICULL — LOCAL AI BIRD PHOTO ORGANIZER (MACOS APPLE SILICON)\n\
                 =======================================================================\n\n\
                 avicull organizes bird photography archives using local neural networks\n\
                 (YOLO11 + BioCLIP), eBird regional checklists, burst consensus smoothing,\n\
                 and generates a standalone offline HTML photo catalog (bird_catalog.html).\n\n\
                 It supports in-place descriptive renaming (default) and zero-copy APFS\n\
                 hard-linked species hierarchies (--mode hierarchy).\n\n\
                 COMMANDS:\n\
                   scan        Pre-flight audit of volume hardlink support and location resolution\n\
                   organize    Scan photo archive, run AI classification, rename in-place or link\n\
                   relink      Re-materialize output hard links from cached SQLite database\n\
                   clean       Losslessly revert in-place renames or unlink generated hard links\n\
                   report      Export species summary counts and review manifests\n\
                   models      Check status of neural weights and eBird taxonomy cache\n\
                   update      Check for newer releases or self-update to latest binary\n\
                   help        Display interactive topic guides and troubleshooting documentation\n\n\
                 FLAGS & OPTIONS:\n\
                   --input <DIR>            Source trip directory\n\
                   --output <DIR>           Target output directory (defaults to <INPUT>/birds_hierarchy in hierarchy mode)\n\
                   --mode <MODE>            Organization mode: 'in-place' (default) or 'hierarchy'\n\
                   --dry-run                Calculate classifications without renaming files or creating hard links\n\
                   --location <NAME>        Override location name or coordinates for photos lacking GPS\n\
                   --min-confidence <FLOAT> Score threshold for Confirmed vs Needs Review (default: 0.65)\n\
                   --min-detect <FLOAT>     YOLO11 bird detection floor (default: 0.25)\n\
                   --burst-window-ms <MS>   Time window for burst clustering in milliseconds (default: 3000)\n\
                   --force                  Re-run neural network inference even if photos were previously classified\n\
                   --no-prompt              Run non-interactively without prompting\n\n\
                 Run 'avicull help <TOPIC>' for deep dives on:\n\
                   quickstart, organize, scan, invariants, models, update\n"
            }
        }
    }
}

/// Returns a list of available topic identifiers and summary descriptions.
pub fn list_topics() -> Vec<(&'static str, &'static str)> {
    vec![
        ("quickstart", "Quickstart guide and basic usage commands"),
        (
            "organize",
            "Deep-dive into the AI detection, classification, and routing pipeline",
        ),
        (
            "scan",
            "Pre-flight audit of volume compatibility and geolocation resolution",
        ),
        (
            "invariants",
            "APFS zero-byte hard links, source immutability, and filesystem safety",
        ),
        (
            "models",
            "YOLO11 detector, BioCLIP classifier, and eBird taxonomy cache",
        ),
        (
            "update",
            "Self-update mechanism, release downloads, and update notifications",
        ),
        (
            "all",
            "Comprehensive user manual and complete CLI reference",
        ),
    ]
}

/// Renders help content for a specific topic, or the overview if None is provided.
pub fn render_help(topic: Option<HelpTopic>) -> String {
    let chosen = topic.unwrap_or(HelpTopic::All);
    let mut out = String::new();
    out.push_str("\n=======================================================================\n");
    out.push_str(&format!("  avicull Help — {}\n", chosen.title()));
    out.push_str("=======================================================================\n\n");
    out.push_str(chosen.content());
    out.push('\n');

    if chosen != HelpTopic::All {
        out.push_str("-----------------------------------------------------------------------\n");
        out.push_str(
            "Available topics: quickstart, organize, scan, invariants, models, update, all\n",
        );
        out.push_str("Run 'avicull help <TOPIC>' to view a specific guide.\n\n");
    }

    out
}

/// Backward compatibility alias for render_help.
pub fn render_guide(topic: Option<HelpTopic>) -> String {
    render_help(topic)
}
