use anyhow::Result;
use avicull::manifest::ManifestDb;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_location_subcommand_updates_sqlite_directory_location() -> Result<()> {
    let dir = tempdir()?;
    let db_dir = dir.path().join(".avicull");
    fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("manifest.sqlite");

    // Initialize database
    let db = ManifestDb::open_or_create(&db_path)?;
    drop(db);

    // Call CLI: avicull location --db <PATH> --dir "Trip_Leh" --location "34.1526, 77.5771"
    let status = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--dir")
        .arg("Trip_Leh")
        .arg("--location")
        .arg("34.1526, 77.5771")
        .status()?;

    assert!(
        status.success(),
        "avicull location with coordinates must succeed"
    );

    // Verify database was updated
    let db = ManifestDb::open_or_create(&db_path)?;
    let loc = db
        .get_directory_location("Trip_Leh")?
        .expect("Trip_Leh location found");
    assert!((loc.latitude - 34.1526).abs() < 1e-4);
    assert!((loc.longitude - 77.5771).abs() < 1e-4);

    // Call CLI with named location: "Leh"
    let status2 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--dir")
        .arg("Trip_Leh")
        .arg("--location")
        .arg("Leh")
        .status()?;

    assert!(
        status2.success(),
        "avicull location with named location must succeed"
    );

    let loc2 = db
        .get_directory_location("Trip_Leh")?
        .expect("Trip_Leh location found");
    assert!(loc2.name.to_lowercase().contains("leh"));

    // Call CLI with image-level override: avicull location --file "IMG_4001" --location "Bangalore"
    // First record run in database so bundles foreign key succeeds
    db.record_run("run_test", "/fake/input", "/fake/output", false, "{}")?;

    let dummy_decision = avicull::types::BundleDecision {
        bundle_id: "bundle_4001".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        classification: avicull::types::Classification::from_slug(&"House_Sparrow".to_string()),
        frame_confidence: 0.95,
        final_confidence: 0.95,
        routing_tier: avicull::types::RoutingTier::ConfirmedSpecies,
        top_predictions: vec![],
        is_user_override: false,
    };

    db.record_bundle(
        "run_test",
        &avicull::types::AssetBundle {
            bundle_id: "bundle_4001".to_string(),
            source_inode: 4001,
            trip_folder_name: "Trip_Leh".to_string(),
            original_stem: "IMG_4001".to_string(),
            primary_image: std::path::PathBuf::from("Trip_Leh/IMG_4001.CR2"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: Some(std::path::PathBuf::from("Trip_Leh/IMG_4001.CR2")),
        },
        &dummy_decision,
    )?;

    let status_file = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg("IMG_4001")
        .arg("--location")
        .arg("Bangalore")
        .status()?;

    assert!(
        status_file.success(),
        "avicull location --file must succeed"
    );

    let bundle_loc = db
        .get_bundle_location("bundle_4001")?
        .expect("Bundle location recorded");
    assert!(bundle_loc.name.to_lowercase().contains("bangalore"));

    Ok(())
}

#[test]
fn test_cli_location_ambiguous_file_fails_with_choices_and_relative_path_disambiguates(
) -> Result<()> {
    let dir = tempdir()?;
    let db_dir = dir.path().join(".avicull");
    fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("manifest.sqlite");

    let db = ManifestDb::open_or_create(&db_path)?;
    db.record_run("run_test", "/fake/input", "/fake/output", false, "{}")?;

    let dummy_decision = avicull::types::BundleDecision {
        bundle_id: "bundle_dubare_trip1".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        classification: avicull::types::Classification::from_slug(&"House_Sparrow".to_string()),
        frame_confidence: 0.95,
        final_confidence: 0.95,
        routing_tier: avicull::types::RoutingTier::ConfirmedSpecies,
        top_predictions: vec![],
        is_user_override: false,
    };

    // First bundle in Trip_A
    db.record_bundle(
        "run_test",
        &avicull::types::AssetBundle {
            bundle_id: "bundle_dubare_trip1".to_string(),
            source_inode: 5001,
            trip_folder_name: "Trip_A".to_string(),
            original_stem: "dubare_19".to_string(),
            primary_image: std::path::PathBuf::from("Trip_A/dubare_19.jpg"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: Some(std::path::PathBuf::from("Trip_A/dubare_19.jpg")),
        },
        &dummy_decision,
    )?;

    // Second bundle in Trip_B with identical stem
    let mut decision2 = dummy_decision.clone();
    decision2.bundle_id = "bundle_dubare_trip2".to_string();
    db.record_bundle(
        "run_test",
        &avicull::types::AssetBundle {
            bundle_id: "bundle_dubare_trip2".to_string(),
            source_inode: 5002,
            trip_folder_name: "Trip_B".to_string(),
            original_stem: "dubare_19".to_string(),
            primary_image: std::path::PathBuf::from("Trip_B/dubare_19.jpg"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: Some(std::path::PathBuf::from("Trip_B/dubare_19.jpg")),
        },
        &decision2,
    )?;
    drop(db);

    // 1. Ambiguous query with bare stem must fail and list both relative paths
    let ambiguous_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg("dubare_19")
        .arg("--location")
        .arg("Bangalore")
        .output()?;

    assert!(
        !ambiguous_output.status.success(),
        "avicull location with ambiguous file must fail"
    );
    let stderr = String::from_utf8_lossy(&ambiguous_output.stderr);
    assert!(
        stderr.contains("Multiple photo bundles matched query 'dubare_19'"),
        "stderr must report ambiguity: {stderr}"
    );
    assert!(
        stderr.contains("Trip_A/dubare_19.jpg") && stderr.contains("Trip_B/dubare_19.jpg"),
        "stderr must list both candidate paths: {stderr}"
    );

    // 2. Disambiguated query using relative path must succeed for Trip_A
    let status_a = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg("Trip_A/dubare_19.jpg")
        .arg("--location")
        .arg("Bangalore")
        .status()?;
    assert!(
        status_a.success(),
        "Disambiguated location for Trip_A must succeed"
    );

    let db = ManifestDb::open_or_create(&db_path)?;
    let loc_a = db
        .get_bundle_location("bundle_dubare_trip1")?
        .expect("Trip_A location updated");
    assert!(loc_a.name.to_lowercase().contains("bangalore"));
    assert!(db.get_bundle_location("bundle_dubare_trip2")?.is_none());

    // 3. Disambiguated query using relative path must succeed for Trip_B
    let status_b = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("location")
        .arg("--db")
        .arg(&db_path)
        .arg("--file")
        .arg("Trip_B/dubare_19.jpg")
        .arg("--location")
        .arg("Leh")
        .status()?;
    assert!(
        status_b.success(),
        "Disambiguated location for Trip_B must succeed"
    );

    let loc_b = db
        .get_bundle_location("bundle_dubare_trip2")?
        .expect("Trip_B location updated");
    assert!(loc_b.name.to_lowercase().contains("leh"));

    Ok(())
}
