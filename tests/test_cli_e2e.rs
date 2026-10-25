use avicull::db::*;
use avicull::organizer::*;
use avicull::scanner::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;

#[test]
fn test_cli_e2e_full_lifecycle() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Trips");
    let output_dir = dir.path().join("Birds_By_Species");

    // Trip 1
    let trip1 = input_dir.join("2025-11_Bharatpur_Keoladeo");
    fs::create_dir_all(&trip1).unwrap();
    let t1_img1 = trip1.join("DSC_4011.JPG");
    let t1_xmp1 = trip1.join("DSC_4011.xmp");
    let t1_raw = trip1.join("DSC_4015.ARW");
    File::create(&t1_img1)
        .unwrap()
        .write_all(b"JPEG 4011")
        .unwrap();
    File::create(&t1_xmp1)
        .unwrap()
        .write_all(b"XMP 4011")
        .unwrap();
    File::create(&t1_raw)
        .unwrap()
        .write_all(b"ARW 4015")
        .unwrap();

    // Trip 2
    let trip2 = input_dir.join("2026-01_Ranganathittu");
    fs::create_dir_all(&trip2).unwrap();
    let t2_img1 = trip2.join("DSC_4011.JPG"); // Same stem as trip 1, different file!
    let t2_xmp1 = trip2.join("DSC_4011.xmp");
    File::create(&t2_img1)
        .unwrap()
        .write_all(b"JPEG 4011 TRIP 2")
        .unwrap();
    File::create(&t2_xmp1)
        .unwrap()
        .write_all(b"XMP 4011 TRIP 2")
        .unwrap();

    let meta_t1 = fs::metadata(&t1_img1).unwrap();
    let meta_t2 = fs::metadata(&t2_img1).unwrap();
    assert_ne!(meta_t1.ino(), meta_t2.ino(), "Different inodes");

    // 1. Scan validation
    validate_volume_and_disjointness(&input_dir, &output_dir).expect("Preflight validation failed");
    let bundles = scan_input_tree(&input_dir).expect("Scan failed");
    assert_eq!(bundles.len(), 3, "Expected 3 bundles total");

    // 2. Mock decision generation
    let run_id = "run-test-e2e";
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db failed");
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();

    let mut decisions = Vec::new();
    for bundle in &bundles {
        let (slug, conf) = if bundle.trip_folder_name.contains("Bharatpur") {
            ("Painted_Stork", 0.96)
        } else {
            ("Painted_Stork", 0.54) // Trip 2 has Painted Stork below 0.65 -> Needs Review
        };

        decisions.push(BundleDecision {
            bundle_id: bundle.bundle_id.clone(),
            burst_id: Some("burst-001".to_string()),
            detection_confidence: 0.95,
            bbox_xyxy_norm: None,
            top_predictions: vec![],
            frame_confidence: conf,
            final_confidence: conf,
            assigned_species_slug: slug.to_string(),
            routing_tier: if conf >= 0.65 {
                RoutingTier::ConfirmedSpecies
            } else {
                RoutingTier::NeedsReview
            },
        });
    }

    db.record_bundles(run_id, &decisions, &bundles).unwrap();

    // 3. Materialize hard links
    let mut all_ops = Vec::new();
    for (bundle, decision) in bundles.iter().zip(decisions.iter()) {
        let ops = materialize_bundle_hardlinks(bundle, decision, &output_dir, run_id, false)
            .expect("materialize failed");
        all_ops.extend(ops);
    }
    db.record_hardlink_operations(&all_ops).unwrap();

    // Verify Confirmed folder for Trip 1
    let linked_t1_img = output_dir.join("Painted_Stork/DSC_4011_0.96.JPG");
    let linked_t1_xmp = output_dir.join("Painted_Stork/DSC_4011_0.96.xmp");
    assert!(
        linked_t1_img.exists(),
        "Trip 1 image should be in Painted_Stork"
    );
    assert!(
        linked_t1_xmp.exists(),
        "Trip 1 xmp should be in Painted_Stork"
    );
    assert_eq!(fs::metadata(&linked_t1_img).unwrap().ino(), meta_t1.ino());
    assert_eq!(fs::metadata(&linked_t1_img).unwrap().nlink(), 2);

    // Verify Needs_Review folder for Trip 2
    let linked_t2_img = output_dir.join("_Needs_Review/Painted_Stork/DSC_4011_0.54.JPG");
    assert!(
        linked_t2_img.exists(),
        "Trip 2 image should be in _Needs_Review"
    );
    assert_eq!(fs::metadata(&linked_t2_img).unwrap().ino(), meta_t2.ino());
    assert_eq!(fs::metadata(&linked_t2_img).unwrap().nlink(), 2);

    // 4. Instant Relink:
    // Suppose on relink, Trip 2 is re-scored or assigned confidence 0.96.
    // Since Painted_Stork already has DSC_4011_0.96.JPG from Trip 1 (with different inode),
    // Trip 2 must be disambiguated with trip prefix: 2026-01_Ranganathittu__DSC_4011_0.96.JPG!
    let cached = db
        .get_all_bundles(Some(run_id))
        .expect("get_all_bundles failed");
    for (mut dec, b) in cached {
        if b.trip_folder_name.contains("Ranganathittu") {
            dec.final_confidence = 0.96;
            dec.routing_tier = RoutingTier::ConfirmedSpecies;
            materialize_bundle_hardlinks(&b, &dec, &output_dir, run_id, false).unwrap();
        }
    }

    let disambiguated_t2 =
        output_dir.join("Painted_Stork/2026-01_Ranganathittu__DSC_4011_0.96.JPG");
    assert!(
        disambiguated_t2.exists(),
        "Relinked colliding file must be disambiguated with trip prefix"
    );

    // 5. Clean
    clean_run_hardlinks(&db, run_id).expect("clean failed");
    assert!(!linked_t1_img.exists(), "Output files must be removed");
    assert_eq!(
        fs::metadata(&t1_img1).unwrap().nlink(),
        1,
        "Input nlink must be restored to 1"
    );
}

#[test]
fn test_models_cli_command_status() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["models", "status"])
        .output()
        .expect("failed to execute avicull models");

    assert!(
        output.status.success(),
        "avicull models status must exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Avian Model & Taxonomy Cache"),
        "stdout must display cache header"
    );
}

#[test]
fn test_help_cli_command() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["help"])
        .output()
        .expect("failed to execute avicull help");

    assert!(
        output.status.success(),
        "avicull help must exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("avicull Help"),
        "Help output must have header"
    );
    assert!(
        stdout.contains("COMMANDS:"),
        "Help must show available commands"
    );
}

#[test]
fn test_help_cli_topic_quickstart() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["help", "quickstart"])
        .output()
        .expect("failed to execute avicull help quickstart");

    assert!(
        output.status.success(),
        "avicull help quickstart must exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("QUICKSTART & BASIC USAGE"),
        "Should contain quickstart section"
    );
}

#[test]
fn test_help_cli_topic_scan() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["help", "scan"])
        .output()
        .expect("failed to execute avicull help scan");

    assert!(output.status.success(), "avicull help scan must succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PRE-FLIGHT ARCHIVE SCANNING"),
        "Should contain scan section"
    );
}

#[test]
fn test_help_cli_topic_update() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["help", "update"])
        .output()
        .expect("failed to execute avicull help update");

    assert!(output.status.success(), "avicull help update must succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("SELF-UPDATE"),
        "Should contain self-update section"
    );
}

#[test]
fn test_help_cli_unknown_topic_exits_nonzero() {
    // An unknown topic must exit with a non-zero code and a helpful error message
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["help", "completely_unknown_topic"])
        .output()
        .expect("failed to execute avicull help unknown");

    assert!(
        !output.status.success(),
        "avicull help with unknown topic must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown help topic") || stderr.contains("Available topics"),
        "Error must guide user to valid topics: {}",
        stderr
    );
}

#[test]
fn test_update_cli_command_check_mode() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args(["update", "--check"])
        .output()
        .expect("failed to execute avicull update --check");

    // In environments with or without network, update --check should either succeed or fail gracefully with network error without panicking
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        assert!(stdout.contains("Checking for updates") || stdout.contains("up to date"));
    } else {
        assert!(stderr.contains("Failed to query") || stderr.contains("Failed to retrieve"));
    }
}

#[test]
fn test_cli_e2e_inplace_default() {
    let tmp = tempdir().expect("create temp dir");
    let input_dir = tmp.path().join("Photos");
    let trip_dir = input_dir.join("2026-07_Leh");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("DSC_0042.JPG");
    let xmp_path = trip_dir.join("DSC_0042.xmp");

    // Copy a real sample bird image fixture so YOLO11 detects a bird
    let fixture_img = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/e2e_samples/leh/sample_chukar.jpg");
    if fixture_img.exists() {
        fs::copy(&fixture_img, &img_path).expect("copy bird fixture image");
    } else {
        let img = image::RgbImage::new(100, 100);
        img.save(&img_path).expect("save test jpeg");
    }
    File::create(&xmp_path)
        .unwrap()
        .write_all(b"<xmp>Leh Test</xmp>")
        .unwrap();

    let fixture_models_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to execute avicull organize");

    assert!(
        output.status.success(),
        "avicull organize in-place default failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db_path = input_dir.join(".avicull/manifest.sqlite");
    assert!(
        db_path.exists(),
        "Manifest sqlite must exist at input root in in-place mode"
    );

    let catalog_path = input_dir.join("catalog.html");
    assert!(
        catalog_path.exists(),
        "catalog.html must exist at input root"
    );
    let catalog_meta = fs::metadata(&catalog_path).expect("catalog.html metadata");
    assert!(catalog_meta.len() > 0, "catalog.html must not be empty");

    assert!(
        !img_path.exists(),
        "Original primary image should have been renamed in-place"
    );
    assert!(
        !xmp_path.exists(),
        "Original sidecar should have been renamed in-place in lockstep"
    );

    let entries: Vec<_> = fs::read_dir(&trip_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let renamed_jpg = entries
        .iter()
        .find(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("DSC_0042_") && name.ends_with(".JPG")
        })
        .expect("Renamed JPG matching DSC_0042_* must exist");
    let renamed_xmp = entries
        .iter()
        .find(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("DSC_0042_") && name.ends_with(".xmp")
        })
        .expect("Renamed xmp matching DSC_0042_* must exist");

    let jpg_stem = renamed_jpg
        .path()
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let xmp_stem = renamed_xmp
        .path()
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        jpg_stem, xmp_stem,
        "Renamed primary and sidecar stems must match"
    );

    let db = ManifestDb::open(&db_path).expect("open manifest db");
    let run_id = db
        .get_latest_run_id()
        .expect("query run_id")
        .expect("latest run_id must exist");

    let clean_output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "clean",
            "--db",
            db_path.to_str().unwrap(),
            "--run-id",
            &run_id,
        ])
        .output()
        .expect("failed to execute avicull clean");

    assert!(
        clean_output.status.success(),
        "avicull clean failed: stderr: {}",
        String::from_utf8_lossy(&clean_output.stderr)
    );

    assert!(
        img_path.exists(),
        "Primary image must be reverted to original name"
    );
    assert!(
        xmp_path.exists(),
        "Sidecar must be reverted to original name"
    );
    assert!(
        !renamed_jpg.path().exists(),
        "Renamed file must no longer exist"
    );
    assert!(
        !renamed_xmp.path().exists(),
        "Renamed sidecar must no longer exist"
    );
}

#[test]
fn test_cli_e2e_hierarchy_mode() {
    let tmp = tempdir().expect("create temp dir");
    let input_dir = tmp.path().join("Photos");
    let output_dir = tmp.path().join("Hierarchy");
    let trip_dir = input_dir.join("2026-07_Leh");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("DSC_0055.JPG");
    let xmp_path = trip_dir.join("DSC_0055.xmp");

    let img = image::RgbImage::new(10, 10);
    img.save(&img_path).expect("save test jpeg");
    File::create(&xmp_path)
        .unwrap()
        .write_all(b"<xmp>Leh Hierarchy</xmp>")
        .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--no-prompt",
        ])
        .output()
        .expect("failed to execute avicull organize hierarchy");

    assert!(
        output.status.success(),
        "avicull organize hierarchy failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(
        db_path.exists(),
        "Manifest sqlite must exist at output root in hierarchy mode"
    );

    let catalog_path = output_dir.join("catalog.html");
    assert!(
        catalog_path.exists(),
        "catalog.html must exist at output root in hierarchy mode"
    );
    let catalog_meta = fs::metadata(&catalog_path).expect("catalog.html metadata");
    assert!(catalog_meta.len() > 0, "catalog.html must not be empty");

    assert!(
        img_path.exists(),
        "Original input file must stay untouched in hierarchy mode"
    );
    assert!(
        xmp_path.exists(),
        "Original sidecar must stay untouched in hierarchy mode"
    );

    let db = ManifestDb::open(&db_path).expect("open manifest db");
    let run_id = db
        .get_latest_run_id()
        .expect("query run_id")
        .expect("latest run_id must exist");

    let clean_output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "clean",
            "--db",
            db_path.to_str().unwrap(),
            "--run-id",
            &run_id,
        ])
        .output()
        .expect("failed to execute avicull clean");

    assert!(
        clean_output.status.success(),
        "avicull clean failed: stderr: {}",
        String::from_utf8_lossy(&clean_output.stderr)
    );

    assert!(
        img_path.exists(),
        "Input primary file must remain intact after clean"
    );
    assert!(
        xmp_path.exists(),
        "Input sidecar must remain intact after clean"
    );
}

#[test]
fn test_cli_e2e_inplace_does_not_rename_no_bird_detected() {
    let tmp = tempdir().expect("create temp dir");
    let input_dir = tmp.path().join("Trip");
    let trip_dir = input_dir.join("2026-07_Landscape");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("DSC_0099.JPG");
    let xmp_path = trip_dir.join("DSC_0099.xmp");

    // Pure black image has 0 bird detections
    let img = image::RgbImage::new(10, 10);
    img.save(&img_path).expect("save test jpeg");
    File::create(&xmp_path)
        .unwrap()
        .write_all(b"<xmp>Empty Landscape</xmp>")
        .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place",
            "--no-prompt",
        ])
        .output()
        .expect("failed to execute avicull organize");

    assert!(
        output.status.success(),
        "avicull organize failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify original files remain untouched and were NOT renamed
    assert!(
        img_path.exists(),
        "Original image DSC_0099.JPG must remain untouched when no bird detected"
    );
    assert!(
        xmp_path.exists(),
        "Original sidecar DSC_0099.xmp must remain untouched when no bird detected"
    );

    // Ensure no files with No_Bird_Detected suffix were created
    let entries: Vec<_> = fs::read_dir(&trip_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        !entries.iter().any(|name| name.contains("No_Bird_Detected")),
        "In-place mode must not rename files to No_Bird_Detected. Entries: {:?}",
        entries
    );
    assert_eq!(
        entries.len(),
        2,
        "Only the original 2 files should exist in directory"
    );
}
