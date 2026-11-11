//! # Integration & Unit Tests for `organize-all` CLI Subcommand
//!
//! Validates argument parsing, defaults, flag overrides, and execution behavior
//! for the `organize-all` batch orchestration command.

use avicull::cli::{Cli, Commands};
use clap::Parser;
use std::path::PathBuf;

#[test]
fn test_cli_parse_organize_all_defaults() {
    let args = Cli::try_parse_from(["avicull", "organize-all"]).unwrap();
    match args.command {
        Commands::OrganizeAll(all_args) => {
            assert_eq!(all_args.input, PathBuf::from("."));
            assert_eq!(all_args.output, None);
            assert!(!all_args.no_parent_catalog);
            assert_eq!(all_args.mode, avicull::types::OrganizationMode::InPlace);
            assert!(!all_args.no_prompt);
            assert!(!all_args.force);
            assert!(!all_args.verbose);
            assert_eq!(all_args.threads, None);
            assert_eq!(all_args.models_dir, None);
            assert_eq!(all_args.default_lat, None);
            assert_eq!(all_args.default_lon, None);
            assert_eq!(all_args.locations_file, None);
            assert!((all_args.min_confidence - 0.65).abs() < f32::EPSILON);
            assert!((all_args.min_detect - 0.25).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Commands::OrganizeAll"),
    }
}

#[test]
fn test_cli_parse_organize_all_custom_arguments() {
    let args = Cli::try_parse_from([
        "avicull",
        "organize-all",
        "--input",
        "/custom/photos",
        "--output",
        "/custom/output",
        "--no-parent-catalog",
        "--mode",
        "hierarchy",
        "--no-prompt",
        "--force",
        "--verbose",
        "--threads",
        "8",
        "--models-dir",
        "/custom/models",
        "--default-lat",
        "34.15",
        "--default-lon",
        "77.57",
        "--locations-file",
        "/custom/locations.toml",
        "--min-confidence",
        "0.85",
        "--min-detect",
        "0.40",
    ])
    .unwrap();

    match args.command {
        Commands::OrganizeAll(all_args) => {
            assert_eq!(all_args.input, PathBuf::from("/custom/photos"));
            assert_eq!(all_args.output, Some(PathBuf::from("/custom/output")));
            assert!(all_args.no_parent_catalog);
            assert_eq!(
                all_args.mode,
                avicull::types::OrganizationMode::HierarchicalHardlinkHierarchy
            );
            assert!(all_args.no_prompt);
            assert!(all_args.force);
            assert!(all_args.verbose);
            assert_eq!(all_args.threads, Some(8));
            assert_eq!(all_args.models_dir, Some(PathBuf::from("/custom/models")));
            assert_eq!(all_args.default_lat, Some(34.15));
            assert_eq!(all_args.default_lon, Some(77.57));
            assert_eq!(
                all_args.locations_file,
                Some(PathBuf::from("/custom/locations.toml"))
            );
            assert!((all_args.min_confidence - 0.85).abs() < 1e-5);
            assert!((all_args.min_detect - 0.40).abs() < 1e-5);
        }
        _ => panic!("Expected Commands::OrganizeAll"),
    }
}

#[test]
fn test_cli_parse_organize_all_shorthands() {
    let args = Cli::try_parse_from([
        "avicull",
        "organize-all",
        "-i",
        "my_input",
        "-o",
        "my_output",
        "-v",
    ])
    .unwrap();

    match args.command {
        Commands::OrganizeAll(all_args) => {
            assert_eq!(all_args.input, PathBuf::from("my_input"));
            assert_eq!(all_args.output, Some(PathBuf::from("my_output")));
            assert!(all_args.verbose);
        }
        _ => panic!("Expected Commands::OrganizeAll"),
    }
}

#[test]
fn test_run_organize_function_interface() {
    let temp = tempfile::tempdir().unwrap();
    let args = avicull::cli::OrganizeArgs {
        input: temp.path().to_path_buf(),
        output: None,
        dry_run: false,
        force: false,
        reclassify: false,
        min_confidence: 0.65,
        min_detect: 0.25,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: avicull::types::OrganizationMode::InPlace,
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
    };
    avicull::organizer::run_organize(&args).unwrap();
    assert!(temp.path().join(".avicull/manifest.sqlite").exists());
    assert!(temp.path().join("catalog.html").exists());
}

#[test]
fn test_organize_all_mock_directory_structure() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Create mock directories
    let trip_a = root.join("Trip Alpha");
    let trip_b = root.join("Trip Beta");
    let empty_trip = root.join("Empty Trip");
    let skipped_hier = root.join("birds_hierarchy");
    let skipped_hidden = root.join(".hidden_trip");
    let skipped_photoslib = root.join("Test.photoslibrary");
    let skipped_eadir = root.join("@eaDir");

    for d in [
        &trip_a,
        &trip_b,
        &empty_trip,
        &skipped_hier,
        &skipped_hidden,
        &skipped_photoslib,
        &skipped_eadir,
    ] {
        std::fs::create_dir_all(d).unwrap();
    }

    // Copy fixture samples into trip_a and trip_b
    std::fs::write(trip_a.join("photo1.jpg"), b"fake_jpeg_1").unwrap();
    std::fs::write(trip_b.join("photo2.jpg"), b"fake_jpeg_2").unwrap();

    let all_args = avicull::cli::OrganizeAllArgs {
        input: root.to_path_buf(),
        output: None,
        no_parent_catalog: false,
        mode: avicull::types::OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        verbose: true,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    };

    avicull::organizer::run_organize_all(&all_args).unwrap();

    // Verify child outputs
    assert!(trip_a.join(".avicull/manifest.sqlite").exists());
    assert!(trip_a.join("catalog.html").exists());
    assert!(trip_b.join(".avicull/manifest.sqlite").exists());
    assert!(trip_b.join("catalog.html").exists());
    assert!(empty_trip.join(".avicull/manifest.sqlite").exists());
    assert!(empty_trip.join("catalog.html").exists());

    // Verify skipped directories were untouched
    assert!(!skipped_hier.join(".avicull").exists());
    assert!(!skipped_hidden.join(".avicull").exists());
    assert!(!skipped_photoslib.join(".avicull").exists());
    assert!(!skipped_eadir.join(".avicull").exists());

    // Verify root rollup catalog
    assert!(root.join(".avicull/manifest.sqlite").exists());
    assert!(root.join("catalog.html").exists());

    // Verify idempotent re-run
    avicull::organizer::run_organize_all(&all_args).unwrap();
}

#[test]
fn test_organize_all_fault_tolerance_on_child_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Directory A: normal valid trip
    let trip_a = root.join("Trip Good");
    // Directory B: will be simulated as an invalid child (e.g. unreadable file or permission issues)
    let trip_b = root.join("Trip Failing");
    // Directory C: normal valid trip after the failing one
    let trip_c = root.join("Trip Also Good");

    std::fs::create_dir_all(&trip_a).unwrap();
    std::fs::create_dir_all(&trip_b).unwrap();
    std::fs::create_dir_all(&trip_c).unwrap();

    std::fs::write(trip_a.join("photo_a.jpg"), b"fake_jpeg_a").unwrap();
    std::fs::write(trip_c.join("photo_c.jpg"), b"fake_jpeg_c").unwrap();

    // In trip_b, create a directory with the name .avicull that is actually a file,
    // or remove write permissions so run_organize will fail.
    // Creating a regular file named `.avicull` inside `trip_b` causes `fs::create_dir_all` inside
    // `ManifestDb::open` or bundle processing to fail.
    std::fs::write(trip_b.join(".avicull"), b"not a directory").unwrap();

    let all_args = avicull::cli::OrganizeAllArgs {
        input: root.to_path_buf(),
        output: None,
        no_parent_catalog: false,
        mode: avicull::types::OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        verbose: false,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    };

    // Should succeed overall because trip_a and trip_c succeed, even though trip_b fails
    let res = avicull::organizer::run_organize_all(&all_args);
    assert!(
        res.is_ok(),
        "Expected organize_all to tolerate child failure, but failed: {res:?}"
    );

    // Trip A and Trip C should have catalogs and manifests
    assert!(trip_a.join(".avicull/manifest.sqlite").exists());
    assert!(trip_a.join("catalog.html").exists());
    assert!(trip_c.join(".avicull/manifest.sqlite").exists());
    assert!(trip_c.join("catalog.html").exists());

    // Root parent catalog should also have been created
    assert!(root.join(".avicull/manifest.sqlite").exists());
    assert!(root.join("catalog.html").exists());
}

#[test]
fn test_organize_all_all_children_failing_returns_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let trip_fail1 = root.join("Fail1");
    let trip_fail2 = root.join("Fail2");
    std::fs::create_dir_all(&trip_fail1).unwrap();
    std::fs::create_dir_all(&trip_fail2).unwrap();

    std::fs::write(trip_fail1.join(".avicull"), b"not a directory").unwrap();
    std::fs::write(trip_fail2.join(".avicull"), b"not a directory").unwrap();

    let all_args = avicull::cli::OrganizeAllArgs {
        input: root.to_path_buf(),
        output: None,
        no_parent_catalog: false,
        mode: avicull::types::OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        verbose: false,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    };

    let res = avicull::organizer::run_organize_all(&all_args);
    assert!(
        res.is_err(),
        "Expected organize_all to fail when ALL child directories fail"
    );
}

#[test]
fn test_help_includes_organize_all() {
    let help = avicull::help::render_help(None);
    assert!(
        help.contains("organize-all"),
        "Expected CLI help reference to document 'organize-all', but got:\n{help}"
    );
}
