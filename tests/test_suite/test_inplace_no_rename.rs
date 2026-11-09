use avicull::types::*;
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
fn test_inplace_mode_keeps_source_files_strictly_untouched() {
    let temp = tempdir().unwrap();
    let fixture_models_dir = setup_fixture_models_dir(&temp);

    let input_dir = temp.path().join("photos");
    let trip_dir = input_dir.join("Leh");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("IMG_9999.JPG");
    let xmp_path = trip_dir.join("IMG_9999.JPG.xmp");

    let valid_jpeg = [
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b,
        0x0c, 0x19, 0x12, 0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20,
        0x24, 0x2e, 0x27, 0x20, 0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1f, 0x27, 0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff,
        0xc0, 0x00, 0x0b, 0x08, 0x00, 0x10, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00,
        0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
        0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0xbf, 0x00, 0xff, 0xd9,
    ];
    File::create(&img_path)
        .unwrap()
        .write_all(&valid_jpeg)
        .unwrap();
    File::create(&xmp_path)
        .unwrap()
        .write_all(b"<xmp>test metadata</xmp>")
        .unwrap();

    let meta_before_img = fs::metadata(&img_path).unwrap();
    let meta_before_xmp = fs::metadata(&xmp_path).unwrap();

    // 1. Run organize in default mode (which must be in-place no rename)
    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize default");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "avicull organize failed: stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Invariant 1: Source files remain completely untouched
    assert!(
        img_path.exists(),
        "Original primary image must remain untouched in in-place mode"
    );
    assert!(
        xmp_path.exists(),
        "Original sidecar must remain untouched in in-place mode"
    );

    let meta_after_img = fs::metadata(&img_path).unwrap();
    let meta_after_xmp = fs::metadata(&xmp_path).unwrap();
    assert_eq!(
        meta_before_img.len(),
        meta_after_img.len(),
        "Image size must not change"
    );
    assert_eq!(
        meta_before_xmp.len(),
        meta_after_xmp.len(),
        "Sidecar size must not change"
    );

    // Invariant 2: No renamed files created in trip directory
    let entries: Vec<String> = fs::read_dir(&trip_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();
    assert_eq!(
        entries.len(),
        2,
        "Trip directory must only contain original files, got: {:?}",
        entries
    );

    // Invariant 3: Manifest and catalog exist inside .avicull and input root
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), ".avicull/manifest.sqlite must exist");

    let catalog_path = input_dir.join("catalog.html");
    assert!(
        catalog_path.exists(),
        "catalog.html must exist at input root"
    );

    let catalog_html = fs::read_to_string(&catalog_path).expect("read catalog");
    assert!(
        catalog_html.contains("IMG_9999.JPG"),
        "Catalog HTML must reference original untouched filename"
    );

    // Verify run mode recorded in SQLite is 'in_place'
    let db = avicull::db::ManifestDb::open_or_create(&db_path).unwrap();
    assert_eq!(
        db.get_latest_run_mode().unwrap(),
        Some(OrganizationMode::InPlace)
    );
}
