//! # Tests for Standalone View Renderer CLI (`render-catalog`) & View-Only Flags
//!
//! Validates:
//! - `avicull render-catalog` generates HTML catalog directly from SQLite manifest with 0 RAW reads.
//! - `avicull catalog` CLI alias parses and functions identically.
//! - Custom output path and custom trip title options are respected.
//! - `--no-thumbnails` suppresses base64 micro-thumbnail data URIs in the generated catalog.
//! - `organize --view-only` updates single directory catalog without scanning or ML inference.
//! - `organize-all --view-only` updates all child and parent catalogs in seconds.

use avicull::catalog::{CATALOG_FILES_DIR, DEFAULT_CATALOG_FILENAME};
use avicull::cli::{Cli, Commands, OrganizeAllArgs, OrganizeArgs, RenderCatalogArgs};
use avicull::organizer::{render_catalog_standalone, run_organize, run_organize_all};
use avicull::types::OrganizationMode;
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_test_organize_args(input: PathBuf) -> OrganizeArgs {
    OrganizeArgs {
        input,
        output: None,
        dry_run: false,
        force: false,
        reclassify: false,
        reclassify_mode: avicull::cli::ReclassifyMode::Fast,
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

fn make_test_organize_all_args(input: PathBuf) -> OrganizeAllArgs {
    OrganizeAllArgs {
        input,
        output: None,
        no_parent_catalog: false,
        mode: OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        reclassify_mode: avicull::cli::ReclassifyMode::Fast,
        view_only: false,
        verbose: false,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    }
}

#[test]
fn test_render_catalog_cli_parsing_and_alias() {
    // Verify render-catalog subcommand parsing
    let cli = Cli::try_parse_from([
        "avicull",
        "render-catalog",
        "--input",
        "/photos/trip",
        "--trip-name",
        "Kaziranga 2024",
        "--no-thumbnails",
    ])
    .expect("parse render-catalog command");

    match cli.command {
        Commands::RenderCatalog(args) => {
            assert_eq!(args.input, PathBuf::from("/photos/trip"));
            assert_eq!(args.trip_name.as_deref(), Some("Kaziranga 2024"));
            assert!(args.no_thumbnails);
            assert!(args.output_file.is_none());
        }
        _ => panic!("Expected RenderCatalog command variant"),
    }

    // Verify 'catalog' alias works identically
    let cli_alias = Cli::try_parse_from([
        "avicull",
        "catalog",
        "--input",
        "/photos/trip",
        "--output-file",
        "/custom/path/catalog.html",
    ])
    .expect("parse catalog alias");

    match cli_alias.command {
        Commands::RenderCatalog(args) => {
            assert_eq!(args.input, PathBuf::from("/photos/trip"));
            assert_eq!(
                args.output_file,
                Some(PathBuf::from("/custom/path/catalog.html"))
            );
            assert!(!args.no_thumbnails);
        }
        _ => panic!("Expected RenderCatalog command variant from alias"),
    }

    // Verify organize --view-only flag
    let cli_org = Cli::try_parse_from(["avicull", "organize", "--input", ".", "--view-only"])
        .expect("parse organize --view-only");
    match cli_org.command {
        Commands::Organize(args) => assert!(args.view_only),
        _ => panic!("Expected Organize command variant"),
    }

    // Verify organize-all --view-only flag
    let cli_org_all =
        Cli::try_parse_from(["avicull", "organize-all", "--input", ".", "--view-only"])
            .expect("parse organize-all --view-only");
    match cli_org_all.command {
        Commands::OrganizeAll(args) => assert!(args.view_only),
        _ => panic!("Expected OrganizeAll command variant"),
    }
}

#[test]
fn test_render_catalog_standalone_generates_html() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_Leh");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("IMG_0001.CR2");
    fs::write(&raw_file, b"fake_cr2_image_payload").expect("write raw file");

    // 1. Initial organize to create manifest
    let org_args = make_test_organize_args(trip_dir.clone());
    run_organize(&org_args).expect("initial organize");

    let manifest_path = trip_dir.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    assert!(manifest_path.exists(), "Manifest must exist");

    // 2. Remove the original catalog.html and delete the raw file
    let default_html = trip_dir.join(DEFAULT_CATALOG_FILENAME);
    if default_html.exists() {
        fs::remove_file(&default_html).expect("remove initial catalog");
    }
    fs::remove_file(&raw_file).expect("remove raw file to prove zero raw reads");

    // 3. Run standalone render-catalog
    let render_args = RenderCatalogArgs {
        input: trip_dir.clone(),
        output_file: None,
        trip_name: None,
        mode: None,
        no_thumbnails: false,
    };

    let result_path = render_catalog_standalone(&render_args).expect("render_catalog_standalone");
    assert_eq!(result_path, default_html);
    assert!(default_html.exists(), "catalog.html must be generated");

    let content = fs::read_to_string(&default_html).expect("read catalog");
    assert!(content.contains("catalog-data"));
    assert!(content.contains("IMG_0001"));
}

#[test]
fn test_render_catalog_custom_title_and_output() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_Custom");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("IMG_0002.CR2");
    fs::write(&raw_file, b"fake_cr2_image_payload_2").expect("write raw file");

    let org_args = make_test_organize_args(trip_dir.clone());
    run_organize(&org_args).expect("initial organize");

    let custom_output = temp.path().join("exports").join("custom_view.html");

    let render_args = RenderCatalogArgs {
        input: trip_dir.clone(),
        output_file: Some(custom_output.clone()),
        trip_name: Some("Custom Himalayas Birding Expedition".to_string()),
        mode: None,
        no_thumbnails: false,
    };

    let result_path = render_catalog_standalone(&render_args).expect("render with custom args");
    assert_eq!(result_path, custom_output);
    assert!(custom_output.exists(), "Custom output path must exist");

    let content = fs::read_to_string(&custom_output).expect("read custom catalog");
    assert!(
        content.contains("Custom Himalayas Birding Expedition"),
        "Catalog must contain custom title"
    );
}

#[test]
fn test_render_catalog_no_thumbnails() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_Thumbnails");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("IMG_0003.CR2");
    fs::write(&raw_file, b"fake_cr2_payload_3").expect("write raw file");

    let org_args = make_test_organize_args(trip_dir.clone());
    run_organize(&org_args).expect("initial organize");

    let render_args = RenderCatalogArgs {
        input: trip_dir.clone(),
        output_file: None,
        trip_name: None,
        mode: None,
        no_thumbnails: true,
    };

    let catalog_path = render_catalog_standalone(&render_args).expect("render no thumbnails");
    let content = fs::read_to_string(&catalog_path).expect("read catalog");

    // With no_thumbnails = true, embedded_thumb_data_uri should not appear in items
    assert!(
        !content.contains("data:image/jpeg;base64"),
        "Catalog must not contain embedded base64 data URIs when thumbnails are suppressed"
    );
}

#[test]
fn test_organize_view_only_flag() {
    let temp = tempdir().expect("create tempdir");
    let trip_dir = temp.path().join("Trip_View_Only");
    fs::create_dir_all(&trip_dir).expect("create trip dir");

    let raw_file = trip_dir.join("IMG_0004.CR2");
    fs::write(&raw_file, b"fake_cr2_payload_4").expect("write raw file");

    let org_args = make_test_organize_args(trip_dir.clone());
    run_organize(&org_args).expect("initial organize");

    let catalog_path = trip_dir.join(DEFAULT_CATALOG_FILENAME);
    assert!(catalog_path.exists());
    fs::remove_file(&catalog_path).expect("remove catalog");

    // Delete raw image to prove view_only does not touch or need raw files
    fs::remove_file(&raw_file).expect("remove raw image");

    // Re-run organize with view_only = true
    let mut view_only_args = make_test_organize_args(trip_dir.clone());
    view_only_args.view_only = true;

    run_organize(&view_only_args).expect("run organize --view-only");
    assert!(
        catalog_path.exists(),
        "catalog.html must be regenerated by --view-only"
    );

    let content = fs::read_to_string(&catalog_path).expect("read catalog");
    assert!(content.contains("IMG_0004"));
}

#[test]
fn test_organize_all_view_only_flag() {
    let temp = tempdir().expect("create tempdir");
    let root = temp.path().join("Archive");
    let child_1 = root.join("Trip_1");
    let child_2 = root.join("Trip_2");
    fs::create_dir_all(&child_1).expect("create child 1");
    fs::create_dir_all(&child_2).expect("create child 2");

    fs::write(child_1.join("IMG_0005.CR2"), b"cr2_5").expect("write img 5");
    fs::write(child_2.join("IMG_0006.CR2"), b"cr2_6").expect("write img 6");

    let org_all_args = make_test_organize_all_args(root.clone());
    run_organize_all(&org_all_args).expect("initial organize-all");

    let parent_catalog = root.join(DEFAULT_CATALOG_FILENAME);
    let child_1_catalog = child_1.join(DEFAULT_CATALOG_FILENAME);
    let child_2_catalog = child_2.join(DEFAULT_CATALOG_FILENAME);

    assert!(parent_catalog.exists());
    assert!(child_1_catalog.exists());
    assert!(child_2_catalog.exists());

    // Remove all catalogs
    fs::remove_file(&parent_catalog).expect("remove parent catalog");
    fs::remove_file(&child_1_catalog).expect("remove child 1 catalog");
    fs::remove_file(&child_2_catalog).expect("remove child 2 catalog");

    // Run organize-all with view_only = true
    let mut view_only_all_args = make_test_organize_all_args(root.clone());
    view_only_all_args.view_only = true;

    run_organize_all(&view_only_all_args).expect("run organize-all --view-only");

    assert!(
        parent_catalog.exists(),
        "Parent catalog.html must be regenerated by organize-all --view-only"
    );
    assert!(
        child_1_catalog.exists(),
        "Child 1 catalog.html must be regenerated"
    );
    assert!(
        child_2_catalog.exists(),
        "Child 2 catalog.html must be regenerated"
    );
}
