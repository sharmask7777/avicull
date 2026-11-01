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
fn test_catalog_filename_is_bird_catalog() {
    let temp = tempdir().unwrap();
    let fixture_models_dir = setup_fixture_models_dir(&temp);

    let input_dir = temp.path().join("photos");
    let trip_dir = input_dir.join("Leh");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("IMG_1001.JPG");
    let mut file = File::create(&img_path).unwrap();
    file.write_all(b"fake jpeg image data").unwrap();

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
        .expect("run avicull organize");

    assert!(output.status.success(), "avicull organize should succeed");

    assert!(
        input_dir.join("bird_catalog.html").exists(),
        "bird_catalog.html must exist at input root"
    );
    assert!(
        !input_dir.join("catalog.html").exists(),
        "catalog.html must NOT exist"
    );
}

#[test]
fn test_catalog_hybrid_rendering_and_thumbnail_fallback() {
    let temp = tempdir().unwrap();
    let fixture_models_dir = setup_fixture_models_dir(&temp);

    let input_dir = temp.path().join("photos");
    let trip_dir = input_dir.join("Leh");
    fs::create_dir_all(&trip_dir).unwrap();

    let img_path = trip_dir.join("IMG_2001.JPG");
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
    let mut file = File::create(&img_path).unwrap();
    file.write_all(&valid_jpeg).unwrap();

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
        .expect("run avicull organize");

    assert!(output.status.success(), "avicull organize should succeed");

    let catalog_path = input_dir.join("bird_catalog.html");
    assert!(catalog_path.exists(), "bird_catalog.html must exist");

    // 1. Verify thumbnail generation in .avicull/previews/<stem>.jpg
    let preview_path = input_dir.join(".avicull/previews/IMG_2001.jpg");
    assert!(
        preview_path.exists(),
        "Thumbnail must exist at .avicull/previews/IMG_2001.jpg"
    );

    let html_content = fs::read_to_string(&catalog_path).expect("read catalog");

    // 2. Verify html contains hybrid attributes: data-local-src, data-thumb-src, data-is-local
    assert!(
        html_content.contains("data-local-src="),
        "Card must contain data-local-src attribute"
    );
    assert!(
        html_content.contains("data-thumb-src="),
        "Card must contain data-thumb-src attribute"
    );
    assert!(
        html_content.contains("data-is-local="),
        "Card must contain data-is-local attribute"
    );
    assert!(
        html_content.contains(".avicull/previews/IMG_2001.jpg"),
        "HTML must reference .avicull/previews/IMG_2001.jpg"
    );

    // 3. Verify onerror fallback handler swaps to dataset.thumbSrc and adds evicted class
    assert!(
        html_content.contains("dataset.thumbSrc"),
        "HTML onerror fallback must reference dataset.thumbSrc"
    );
    assert!(
        html_content.contains("evicted"),
        "HTML onerror fallback must apply evicted class"
    );

    // 4. Verify apply_corrections.sh includes fileproviderctl evict check
    assert!(
        html_content.contains("fileproviderctl evict"),
        "Correction script generator must include fileproviderctl evict safeguard"
    );
}
