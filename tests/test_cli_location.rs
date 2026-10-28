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

    Ok(())
}
