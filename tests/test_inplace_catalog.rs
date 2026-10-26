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
