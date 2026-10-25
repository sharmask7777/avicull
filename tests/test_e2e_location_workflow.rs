use avicull::db::ManifestDb;
use avicull::types::LocationSource;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_organize_with_location_flag_and_gazetteer() {
    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("trip_bangalore");
    let subfolder = input_dir.join("saul kere apr 19 2026");
    fs::create_dir_all(&subfolder).unwrap();

    // Create dummy image bundle
    fs::write(subfolder.join("photo1.jpg"), b"fake_jpeg").unwrap();

    // Run avicull organize with --dry-run, --location "Bangalore", and --no-prompt
    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--dry-run",
            "--location",
            "Bangalore",
            "--no-prompt",
        ])
        .output()
        .expect("failed to execute avicull organize");

    assert!(
        output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db_path = input_dir.join("birds_hierarchy/.avicull/manifest.sqlite");
    assert!(db_path.exists(), "manifest.sqlite should exist");

    let db = ManifestDb::open(&db_path).expect("open manifest db");
    let bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(bundles.len(), 1);

    let (_decision, bundle) = &bundles[0];
    let gps = bundle.gps_coords.expect("GPS coords should be resolved");
    assert_eq!(gps.source, LocationSource::TripFolderGazetteer);
    assert!((gps.latitude - 12.9716).abs() < 0.1);
    assert!((gps.longitude - 77.5946).abs() < 0.1);

    // Verify directory location in sqlite has region_code IN-KA
    let dir_loc = db
        .get_directory_location(&bundle.trip_folder_name)
        .expect("query directory location")
        .expect("directory location must exist");
    assert_eq!(dir_loc.region_code, "IN-KA");
}

#[test]
fn test_organize_with_path_inference_and_no_prompt() {
    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("wildlife_trip");
    let subfolder = input_dir.join("saul kere apr 19 2026");
    fs::create_dir_all(&subfolder).unwrap();

    fs::write(subfolder.join("photo2.jpg"), b"fake_jpeg").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--dry-run",
            "--no-prompt",
        ])
        .output()
        .expect("failed to execute avicull organize");

    assert!(
        output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db_path = input_dir.join("birds_hierarchy/.avicull/manifest.sqlite");
    assert!(db_path.exists(), "manifest.sqlite should exist");

    let db = ManifestDb::open(&db_path).expect("open manifest db");
    let bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(bundles.len(), 1);

    let (_decision, bundle) = &bundles[0];
    let gps = bundle.gps_coords.expect("GPS coords should be resolved");
    assert_eq!(gps.source, LocationSource::TripFolderGazetteer);
    assert!((gps.latitude - 12.9123).abs() < 0.1);
    assert!((gps.longitude - 77.6845).abs() < 0.1);

    let dir_loc = db
        .get_directory_location(&bundle.trip_folder_name)
        .expect("query directory location")
        .expect("directory location must exist");
    assert_eq!(dir_loc.region_code, "IN-KA");
}

#[test]
fn test_regional_prior_inference_with_resolved_region() {
    use avicull::classifier::{OrtSpeciesClassifier, SpeciesClassifier};
    use avicull::types::GpsCoordinate;
    use std::path::Path;

    let classifier = OrtSpeciesClassifier::new(Path::new("models")).unwrap();
    let dummy_img = image::DynamicImage::new_rgb8(224, 224);
    let coords = Some(GpsCoordinate {
        latitude: 12.9716,
        longitude: 77.5946,
        source: LocationSource::TripFolderGazetteer,
    });

    let (_, preds) = classifier
        .classify_with_region(&dummy_img, coords, Some("IN-KA"), 1.0)
        .expect("classification with region");

    assert!(!preds.is_empty());
    assert_eq!(preds[0].taxon_id, "paisto1");
    assert!(preds[0].combined_score > 0.80);
}
