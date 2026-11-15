//! # Tests for Fast Software Re-classification (`--reclassify-mode fast`)
//!
//! Validates:
//! - Default `ReclassifyMode` is `Fast` across `OrganizeArgs`, `OrganizeAllArgs`, and `ReclassifyArgs`.
//! - Running `organize --reclassify --reclassify-mode fast` or `reclassify` re-evaluates confidence
//!   thresholds and updates routing tiers directly from SQLite with zero RAW file reads.
//! - Standalone `reclassify` CLI subcommand execution operates without raw image files.

use avicull::catalog::{CATALOG_FILES_DIR, DEFAULT_CATALOG_FILENAME};
use avicull::cli::{Cli, Commands, OrganizeArgs, ReclassifyMode};
use avicull::db::ManifestDb;
use avicull::organizer::run_organize;
use avicull::types::{
    AssetCategory, Classification, OrganizationMode, RoutingTier, SpeciesPrediction,
};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn make_test_organize_args(input: PathBuf) -> OrganizeArgs {
    OrganizeArgs {
        input,
        output: None,
        dry_run: false,
        force: false,
        reclassify: false,
        reclassify_mode: ReclassifyMode::Fast,
        view_only: false,
        min_confidence: 0.65,
        min_detect: 0.25,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: OrganizationMode::InPlace,
        geo_prior_weight: 1.0,
        locations_file: None,
        default_lat: None,
        default_lon: None,
        models_dir: None,
        threads: Some(1),
        location: None,
        no_prompt: true,
        max_cache_size: "5GB".to_string(),
        verbose: false,
    }
}

#[test]
fn test_reclassify_mode_enum_default_is_fast() {
    assert_eq!(ReclassifyMode::default(), ReclassifyMode::Fast);

    let cli_org = Cli::try_parse_from(["avicull", "organize", "--input", "."])
        .expect("parse organize default");
    match cli_org.command {
        Commands::Organize(args) => {
            assert_eq!(args.reclassify_mode, ReclassifyMode::Fast);
            assert!(!args.reclassify);
        }
        _ => panic!("Expected Organize variant"),
    }

    let cli_org_forced = Cli::try_parse_from([
        "avicull",
        "organize",
        "--input",
        ".",
        "--reclassify-mode",
        "forced",
    ])
    .expect("parse organize forced");
    match cli_org_forced.command {
        Commands::Organize(args) => {
            assert_eq!(args.reclassify_mode, ReclassifyMode::Forced);
        }
        _ => panic!("Expected Organize variant"),
    }

    let cli_all = Cli::try_parse_from(["avicull", "organize-all", "--input", "."])
        .expect("parse organize-all default");
    match cli_all.command {
        Commands::OrganizeAll(args) => {
            assert_eq!(args.reclassify_mode, ReclassifyMode::Fast);
        }
        _ => panic!("Expected OrganizeAll variant"),
    }

    let cli_reclass = Cli::try_parse_from(["avicull", "reclassify", "--input", "."])
        .expect("parse reclassify default");
    match cli_reclass.command {
        Commands::Reclassify(args) => {
            assert_eq!(args.reclassify_mode, ReclassifyMode::Fast);
            assert!((args.min_confidence - 0.65).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Reclassify variant"),
    }
}

#[test]
fn test_fast_reclassify_updates_routing_tier_without_raw_reads() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_Fast_Reclassify");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("DSC_0001.JPG");
    fs::write(&raw_file, b"test_jpeg_bytes").expect("write raw photo");

    // 1. Initial organize to create manifest and catalog files with min_confidence = 0.85
    let mut org_args = make_test_organize_args(trip_dir.clone());
    org_args.min_confidence = 0.85;
    run_organize(&org_args).expect("initial organize");

    let db_path = trip_dir.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    assert!(db_path.exists(), "Manifest must exist");

    // Seed bundle with 0.70 confidence prediction
    let db = ManifestDb::open_or_create(&db_path).expect("open db");
    let initial_bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(initial_bundles.len(), 1);
    let (mut initial_dec, _) = initial_bundles[0].clone();

    initial_dec.detection_confidence = 0.90;
    initial_dec.top_predictions = vec![SpeciesPrediction {
        taxon_id: "paisto1".to_string(),
        common_name: "Painted Stork".to_string(),
        scientific_name: "Mycteria leucocephala".to_string(),
        folder_slug: "Painted_Stork".to_string(),
        visual_prob: 0.70,
        geo_prior: 1.0,
        combined_score: 0.70,
    }];
    initial_dec.frame_confidence = 0.70;
    initial_dec.final_confidence = 0.70;
    initial_dec.classification = Classification::from_slug("Painted_Stork");
    initial_dec.routing_tier = RoutingTier::NeedsReview;

    let dummy_embedding = vec![0.1f32; 512];
    db.insert_bundle_decision(&initial_dec, &AssetCategory::Bird, Some(&dummy_embedding))
        .expect("update initial bundle with 0.70 confidence");

    // Verify it is currently in NeedsReview
    let pre_check = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(pre_check[0].0.routing_tier, RoutingTier::NeedsReview);

    // 2. Delete the raw source image to strictly prove ZERO raw file reads
    fs::remove_file(&raw_file).expect("remove raw source file");
    assert!(!raw_file.exists(), "Raw source file must be gone");

    // 3. Run fast reclassify with min_confidence = 0.65
    let mut reclass_args = make_test_organize_args(trip_dir.clone());
    reclass_args.reclassify = true;
    reclass_args.reclassify_mode = ReclassifyMode::Fast;
    reclass_args.min_confidence = 0.65;

    run_organize(&reclass_args).expect("fast reclassify must succeed with zero raw reads");

    // 4. Verify that bundle decision was updated to ConfirmedSpecies
    let post_bundles = db
        .get_all_bundles(None)
        .expect("get bundles post reclassify");
    assert_eq!(post_bundles.len(), 1);
    let (post_dec, _) = &post_bundles[0];
    assert_eq!(
        post_dec.routing_tier,
        RoutingTier::ConfirmedSpecies,
        "Bundle decision must update from NeedsReview to ConfirmedSpecies"
    );
    assert_eq!(post_dec.assigned_species_slug(), "Painted_Stork");
    assert!(
        !raw_file.exists(),
        "Raw file was never recreated or touched"
    );

    // 5. Verify catalog.html exists and was regenerated
    let catalog_html = trip_dir.join(DEFAULT_CATALOG_FILENAME);
    assert!(catalog_html.exists(), "catalog.html must be regenerated");
    let content = fs::read_to_string(&catalog_html).expect("read catalog.html");
    assert!(content.contains("Painted Stork"));
}

#[test]
fn test_reclassify_command_cli_execution() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_Cli_Reclassify");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("DSC_0002.JPG");
    fs::write(&raw_file, b"test_jpeg_cli_bytes").expect("write raw photo");

    // 1. Initial organize with min_confidence = 0.85
    let mut org_args = make_test_organize_args(trip_dir.clone());
    org_args.min_confidence = 0.85;
    run_organize(&org_args).expect("initial organize");

    let db_path = trip_dir.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db");
    let initial_bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(initial_bundles.len(), 1);
    let (mut initial_dec, _) = initial_bundles[0].clone();

    initial_dec.detection_confidence = 0.90;
    initial_dec.top_predictions = vec![SpeciesPrediction {
        taxon_id: "paisto1".to_string(),
        common_name: "Painted Stork".to_string(),
        scientific_name: "Mycteria leucocephala".to_string(),
        folder_slug: "Painted_Stork".to_string(),
        visual_prob: 0.70,
        geo_prior: 1.0,
        combined_score: 0.70,
    }];
    initial_dec.frame_confidence = 0.70;
    initial_dec.final_confidence = 0.70;
    initial_dec.classification = Classification::from_slug("Painted_Stork");
    initial_dec.routing_tier = RoutingTier::NeedsReview;

    let dummy_embedding = vec![0.1f32; 512];
    db.insert_bundle_decision(&initial_dec, &AssetCategory::Bird, Some(&dummy_embedding))
        .expect("insert bundle decision");

    // Delete raw photo to ensure zero raw reads during CLI command
    fs::remove_file(&raw_file).expect("remove raw file");
    assert!(!raw_file.exists());

    // 2. Execute `avicull reclassify --input <DIR> --min-confidence 0.65`
    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "reclassify",
            "--input",
            trip_dir.to_str().unwrap(),
            "--min-confidence",
            "0.65",
            "--no-prompt",
        ])
        .output()
        .expect("run avicull reclassify");

    assert!(
        output.status.success(),
        "reclassify CLI command failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 3. Verify manifest updated to ConfirmedSpecies
    let post_bundles = db.get_all_bundles(None).expect("get post bundles");
    assert_eq!(
        post_bundles[0].0.routing_tier,
        RoutingTier::ConfirmedSpecies,
        "CLI reclassify must update routing tier to ConfirmedSpecies"
    );

    // 4. Verify catalog.html updated
    let catalog_html = trip_dir.join(DEFAULT_CATALOG_FILENAME);
    assert!(catalog_html.exists());
}
