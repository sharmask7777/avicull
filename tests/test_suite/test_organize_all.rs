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
