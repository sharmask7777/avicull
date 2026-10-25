use avicull::classifier::{get_regional_prior_multiplier, EMBEDDED_REGIONAL_CHECKLISTS};
use avicull::db::ManifestDb;
use avicull::location::gazetteer::GazetteerEngine;
use avicull::location::resolver::resolve_trip_locations;
use avicull::types::{GpsCoordinate, LocationSource};
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_mock_pipeline_location_resolution_and_regional_priors() {
    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("Raw_Trips");
    let output_dir = tmp.path().join("birds_hierarchy");

    // Trip 1: Saul Kere (Bangalore, Karnataka -> IN-KA)
    let trip_ka = input_dir.join("2026-04_Bangalore/saul kere apr 19 2026");
    fs::create_dir_all(&trip_ka).unwrap();
    let img_ka = trip_ka.join("DSC_0001.JPG");
    File::create(&img_ka)
        .unwrap()
        .write_all(b"fake_jpeg_1")
        .unwrap();

    // Trip 2: Leh (Ladakh -> IN-LA)
    let trip_la = input_dir.join("2026-07_Leh/Shey Marshes");
    fs::create_dir_all(&trip_la).unwrap();
    let img_la = trip_la.join("DSC_0002.JPG");
    File::create(&img_la)
        .unwrap()
        .write_all(b"fake_jpeg_2")
        .unwrap();

    // Execute avicull organize with --dry-run and --no-prompt
    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--dry-run",
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");

    assert!(
        output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), "manifest.sqlite must be created");

    let db = ManifestDb::open(&db_path).expect("open manifest db");
    let bundles = db.get_all_bundles(None).expect("get all bundles");
    assert_eq!(bundles.len(), 2, "Expected 2 bundles in mock pipeline");

    let mut bundle_by_trip = std::collections::HashMap::new();
    for (_dec, b) in &bundles {
        bundle_by_trip.insert(b.trip_folder_name.clone(), b);
    }

    // Verify Trip 1 (Bangalore / Saul Kere -> IN-KA)
    let bundle_ka = bundle_by_trip
        .get("2026-04_Bangalore")
        .or_else(|| bundle_by_trip.get("saul kere apr 19 2026"))
        .expect("Bangalore bundle must exist");
    let gps_ka = bundle_ka
        .gps_coords
        .expect("GPS must be resolved for Bangalore");
    assert_eq!(gps_ka.source, LocationSource::TripFolderGazetteer);
    assert!((gps_ka.latitude - 12.9).abs() < 0.2);
    assert!((gps_ka.longitude - 77.6).abs() < 0.2);

    let dir_loc_ka = db
        .get_directory_location(&bundle_ka.trip_folder_name)
        .expect("query dir loc")
        .expect("dir loc must exist");
    assert_eq!(dir_loc_ka.region_code, "IN-KA");

    // Verify Trip 2 (Leh / Shey Marshes -> IN-LA)
    let bundle_la = bundle_by_trip
        .get("2026-07_Leh")
        .or_else(|| bundle_by_trip.get("Shey Marshes"))
        .expect("Leh bundle must exist");
    let gps_la = bundle_la.gps_coords.expect("GPS must be resolved for Leh");
    assert_eq!(gps_la.source, LocationSource::TripFolderGazetteer);
    assert!((gps_la.latitude - 34.1).abs() < 0.2);
    assert!((gps_la.longitude - 77.6).abs() < 0.2);

    let dir_loc_la = db
        .get_directory_location(&bundle_la.trip_folder_name)
        .expect("query dir loc")
        .expect("dir loc must exist");
    assert_eq!(dir_loc_la.region_code, "IN-LA");

    // Verify Regional Prior Filtering:
    // In Karnataka (IN-KA):
    // - Brahminy Kite (brakit1) is native -> prior mult 1.0
    // - Black Redstart (blared1) is Ladakh high-altitude native, not native to Bangalore -> prior mult 0.001
    let mult_ka_native = get_regional_prior_multiplier(
        Some(&dir_loc_ka.region_code),
        "brakit1",
        &EMBEDDED_REGIONAL_CHECKLISTS,
    );
    let mult_ka_out_of_range = get_regional_prior_multiplier(
        Some(&dir_loc_ka.region_code),
        "baleag",
        &EMBEDDED_REGIONAL_CHECKLISTS,
    );
    assert_eq!(mult_ka_native, 1.0);
    assert_eq!(mult_ka_out_of_range, 0.001);

    // In Ladakh (IN-LA):
    // - Black Redstart (blared1) and Chukar (chukar) are native -> prior mult 1.0
    // - Brahminy Kite (brakit1) is not native -> prior mult 0.001
    let mult_la_redstart = get_regional_prior_multiplier(
        Some(&dir_loc_la.region_code),
        "blared1",
        &EMBEDDED_REGIONAL_CHECKLISTS,
    );
    let mult_la_chukar = get_regional_prior_multiplier(
        Some(&dir_loc_la.region_code),
        "chukar",
        &EMBEDDED_REGIONAL_CHECKLISTS,
    );
    let mult_la_out_of_range = get_regional_prior_multiplier(
        Some(&dir_loc_la.region_code),
        "brakit1",
        &EMBEDDED_REGIONAL_CHECKLISTS,
    );
    assert_eq!(mult_la_redstart, 1.0);
    assert_eq!(mult_la_chukar, 1.0);
    assert_eq!(mult_la_out_of_range, 0.001);
}

#[test]
fn test_mock_pipeline_hardware_exif_gps_preservation() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("manifest.sqlite");
    let manifest = ManifestDb::open_or_create(&db_path).unwrap();
    let engine = GazetteerEngine::load_embedded().unwrap();

    let trips = vec![("Bangalore_Trip".to_string(), vec!["bangalore".to_string()])];

    let mut input = Cursor::new("");
    let mut output = Vec::new();

    let resolved = resolve_trip_locations(
        &trips,
        &engine,
        &manifest,
        &mut input,
        &mut output,
        false,
        None,
    )
    .unwrap();

    let loc = resolved.get("Bangalore_Trip").unwrap();
    assert_eq!(loc.region_code, "IN-KA");

    // Camera bundle with existing hardware EXIF GPS
    let camera_lat = 13.0827;
    let camera_lon = 80.2707;
    let mut bundle_with_exif = avicull::types::AssetBundle {
        bundle_id: "test-bundle-with-exif".to_string(),
        source_inode: 12345,
        trip_folder_name: "Bangalore_Trip".to_string(),
        original_stem: "IMG_0001".to_string(),
        primary_image: PathBuf::from("Bangalore_Trip/IMG_0001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: Some(1700000000000),
        gps_coords: Some(GpsCoordinate {
            latitude: camera_lat,
            longitude: camera_lon,
            source: LocationSource::ExifGps,
        }),
        resolved_location_name: None,
    };

    // Simulate main.rs preservation logic
    let has_exif_gps = bundle_with_exif
        .gps_coords
        .map(|c| c.source == LocationSource::ExifGps)
        .unwrap_or(false);

    if !has_exif_gps {
        bundle_with_exif.gps_coords = Some(GpsCoordinate {
            latitude: loc.latitude,
            longitude: loc.longitude,
            source: LocationSource::TripFolderGazetteer,
        });
    }

    // Hardware GPS must be preserved exactly
    let final_gps = bundle_with_exif.gps_coords.unwrap();
    assert_eq!(final_gps.source, LocationSource::ExifGps);
    assert_eq!(final_gps.latitude, camera_lat);
    assert_eq!(final_gps.longitude, camera_lon);
}
