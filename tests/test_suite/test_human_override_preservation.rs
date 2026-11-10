//! # Integration Test: Human Override Preservation & Reclassify
//!
//! Verifies:
//! 1. Non-destructive mode (`--mode in-place`) preserves human-overridden classifications
//!    when re-running `organize` without `--reclassify`.
//! 2. Supplying the `--reclassify` flag forces full neural re-detection and re-classification
//!    across the entire archive (doing the entire thing again).

use avicull::db::ManifestDb;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn setup_fixture_models_dir(temp_dir: &tempfile::TempDir) -> PathBuf {
    let models_dir = temp_dir.path().join("models");
    fs::create_dir_all(&models_dir).unwrap();
    File::create(models_dir.join("detector.onnx")).unwrap();
    File::create(models_dir.join("classifier.onnx")).unwrap();
    File::create(models_dir.join("species_map.json")).unwrap();
    File::create(models_dir.join("species_meta.json")).unwrap();
    File::create(models_dir.join("spatial_priors.bin")).unwrap();
    File::create(models_dir.join("gazetteer.bin")).unwrap();
    models_dir
}

#[test]
fn test_organize_preserves_human_overrides_unless_reclassify_passed() {
    let temp = tempdir().unwrap();
    let fixture_models = setup_fixture_models_dir(&temp);

    let input_dir = temp.path().join("photos");
    fs::create_dir_all(&input_dir).unwrap();

    let img_path = input_dir.join("IMG_2001.JPG");
    let mut file = File::create(&img_path).unwrap();
    file.write_all(b"sample test jpeg").unwrap();

    // 1. Initial organize run (In-Place non-destructive mode)
    let status1 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--models-dir",
            fixture_models.to_str().unwrap(),
            "--mode",
            "in-place",
            "--no-prompt",
        ])
        .status()
        .expect("run organize initial");
    assert!(status1.success(), "Initial organize must succeed");

    // Check manifest exists
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), "Manifest database must exist");

    // 2. Simulate human override on the bundle
    let db = ManifestDb::open_or_create(&db_path).expect("open manifest");
    let all_bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(all_bundles.len(), 1, "Must have exactly 1 bundle recorded");
    let (initial_dec, initial_bundle) = &all_bundles[0];
    assert!(
        !initial_dec.is_user_override,
        "Initial decision must not be an override"
    );

    // Apply human override: change species to "Golden Eagle" with is_user_override = 1
    db.update_bundle_override(
        &initial_bundle.bundle_id,
        "Golden_Eagle",
        avicull::types::RoutingTier::ConfirmedSpecies,
        1.0,
    )
    .expect("update override");

    // Verify override is set in SQLite
    let bundles_after_override = db.get_all_bundles(None).expect("get bundles");
    let (overridden_dec, _) = &bundles_after_override[0];
    assert!(
        overridden_dec.is_user_override,
        "Override must be recorded in SQLite"
    );
    assert_eq!(overridden_dec.assigned_species_slug, "Golden_Eagle");

    // 3. Re-run organize in non-destructive mode WITHOUT --reclassify
    let status2 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--models-dir",
            fixture_models.to_str().unwrap(),
            "--mode",
            "in-place",
            "--no-prompt",
        ])
        .status()
        .expect("run organize second time");
    assert!(status2.success(), "Second organize must succeed");

    // Verify human override was PRESERVED in SQLite manifest
    let bundles_after_rerun = db.get_all_bundles(None).expect("get bundles");
    let (preserved_dec, _) = &bundles_after_rerun[0];
    assert!(
        preserved_dec.is_user_override,
        "User override must be preserved on normal organize re-run"
    );
    assert_eq!(
        preserved_dec.assigned_species_slug, "Golden_Eagle",
        "Species must remain Golden Eagle"
    );

    // Verify catalog.html reflects the preserved Golden Eagle override
    let catalog_content =
        fs::read_to_string(input_dir.join("catalog.html")).expect("read catalog.html");
    assert!(
        catalog_content.contains("Golden Eagle"),
        "catalog.html must reflect the preserved human override"
    );

    // 4. Re-run organize WITH --reclassify (should redo the entire thing again)
    let status3 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--models-dir",
            fixture_models.to_str().unwrap(),
            "--mode",
            "in-place",
            "--reclassify",
            "--no-prompt",
        ])
        .status()
        .expect("run organize with --reclassify");
    assert!(status3.success(), "Reclassify organize must succeed");

    // Verify that with --reclassify, the entire thing was re-done and previous override reset
    let bundles_after_reclassify = db.get_all_bundles(None).expect("get bundles");
    let (reclassified_dec, _) = &bundles_after_reclassify[0];
    assert!(
        !reclassified_dec.is_user_override,
        "--reclassify must re-evaluate from scratch and reset previous override"
    );
}
