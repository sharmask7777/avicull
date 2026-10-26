use avicull::db::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_correct_subcommand_updates_manifest_and_hardlink() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0001.JPG");
    let sidecar = trip_dir.join("DSC_0001.xmp");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_0001 DATA")
        .unwrap();
    File::create(&sidecar)
        .unwrap()
        .write_all(b"<xmp>DSC_0001 SIDECAR</xmp>")
        .unwrap();

    let src_meta = fs::metadata(&photo).unwrap();
    let src_ino = src_meta.ino();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Run avicull organize in hierarchy mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");

    assert!(
        org_output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), "manifest.sqlite should exist");

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles.len(), 1);
    let initial_slug = bundles[0].0.assigned_species_slug.clone();
    assert_ne!(initial_slug, "Painted_Stork");

    // 2. Call avicull correct --file "DSC_0001" --species "Painted Stork"
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(
        correct_output.status.success(),
        "avicull correct failed: stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // 3. Verify SQLite record has is_user_override = 1, slug = "Painted_Stork"
    let updated_bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(updated_bundles.len(), 1);
    let (decision, _bundle) = &updated_bundles[0];
    assert_eq!(decision.assigned_species_slug, "Painted_Stork");
    assert_eq!(decision.routing_tier, RoutingTier::ConfirmedSpecies);
    assert!(
        (decision.final_confidence - 1.0).abs() < f32::EPSILON,
        "final_confidence should be 1.0"
    );
    assert!(decision.is_user_override, "is_user_override must be true");

    // 4. Verify hardlink exists under birds_hierarchy/Painted_Stork/
    let new_species_dir = output_dir.join("Painted_Stork");
    assert!(
        new_species_dir.exists(),
        "Painted_Stork directory must exist"
    );
    let entries: Vec<_> = fs::read_dir(&new_species_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let new_photo_link = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("DSC_0001") && name.ends_with(".JPG")
        })
        .expect("Photo hard link should exist under Painted_Stork/");
    let _new_sidecar_link = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("DSC_0001") && name.ends_with(".xmp")
        })
        .expect("Sidecar hard link should exist under Painted_Stork/");

    let dst_meta = fs::metadata(new_photo_link).unwrap();
    assert_eq!(
        dst_meta.ino(),
        src_ino,
        "Hard link inode must match source inode"
    );

    // Strict Source Immutability
    let src_meta_after = fs::metadata(&photo).unwrap();
    assert_eq!(
        src_meta_after.ino(),
        src_ino,
        "Source file inode must remain unchanged"
    );

    // 5. Verify catalog.html is regenerated with new species
    let catalog_path = output_dir.join("catalog.html");
    assert!(catalog_path.exists(), "catalog.html should exist");
    let catalog_content = fs::read_to_string(&catalog_path).unwrap();
    assert!(
        catalog_content.contains("Painted_Stork") || catalog_content.contains("Painted Stork"),
        "catalog.html should contain updated species 'Painted_Stork'"
    );
}

#[test]
fn test_cli_correct_preserves_override_on_subsequent_organize_rerun() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0002.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_0002 DATA")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Initial organize
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    // 2. Set manual correction
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0002.JPG",
            "--species",
            "Black-necked Stork",
        ])
        .output()
        .expect("failed to run avicull correct");
    assert!(correct_output.status.success());

    // 3. Add a new photo to force organize to re-cluster and re-smooth
    let photo2 = trip_dir.join("DSC_0003.JPG");
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG DSC_0003 DATA")
        .unwrap();

    let rerun_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize re-run");
    assert!(rerun_output.status.success());

    // 4. Verify DSC_0002 still has Black-necked Stork override!
    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();

    let b2 = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0002")
        .expect("DSC_0002 must exist in manifest");

    assert_eq!(
        b2.0.assigned_species_slug, "Black-necked_Stork",
        "Override species slug must be preserved across organize re-runs"
    );
    assert!(
        b2.0.is_user_override,
        "is_user_override must remain true across organize re-runs"
    );
    assert_eq!(
        b2.0.routing_tier,
        RoutingTier::ConfirmedSpecies,
        "Routing tier must remain ConfirmedSpecies"
    );
}

#[test]
fn test_cli_correct_nonexistent_file_exits_with_error() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    fs::create_dir_all(&input_dir).unwrap();

    let photo = input_dir.join("DSC_0001.JPG");
    File::create(&photo).unwrap().write_all(b"JPEG").unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // Organize
    Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .unwrap();

    // Call correct on nonexistent file
    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "NONEXISTENT_STEM",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to execute avicull correct");

    assert!(
        !output.status.success(),
        "avicull correct should exit nonzero on missing bundle"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NONEXISTENT_STEM") || stderr.contains("not found"),
        "stderr should mention the missing file/stem: {}",
        stderr
    );
}

#[test]
fn test_cli_correct_inplace_mode_updates_manifest_and_renames() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0005.JPG");
    let sidecar = trip_dir.join("DSC_0005.xmp");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_0005 DATA")
        .unwrap();
    File::create(&sidecar)
        .unwrap()
        .write_all(b"<xmp>DSC_0005 SIDECAR</xmp>")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Initial organize in in-place mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize in-place");
    assert!(org_output.status.success());

    // 2. Correct to Painted Stork
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place",
            "--file",
            "DSC_0005",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct in-place");
    assert!(
        correct_output.status.success(),
        "avicull correct in-place failed: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // 3. Verify files were renamed on disk with Painted_Stork
    let entries: Vec<_> = fs::read_dir(&trip_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        entries.iter().any(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.contains("DSC_0005") && name.contains("Painted_Stork") && name.ends_with(".JPG")
        }),
        "Renamed photo with Painted_Stork should exist"
    );
    assert!(
        entries.iter().any(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.contains("DSC_0005") && name.contains("Painted_Stork") && name.ends_with(".xmp")
        }),
        "Renamed sidecar with Painted_Stork should exist"
    );

    // 4. Verify SQLite record
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    let b5 = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0005")
        .expect("DSC_0005 must exist in manifest");
    assert_eq!(b5.0.assigned_species_slug, "Painted_Stork");
    assert!(b5.0.is_user_override);
    assert_eq!(b5.0.routing_tier, RoutingTier::ConfirmedSpecies);
}

#[test]
fn test_cli_correct_ambiguous_file_fails_with_choices() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip1_dir = input_dir.join("Bharatpur");
    let trip2_dir = input_dir.join("Corbett");
    fs::create_dir_all(&trip1_dir).unwrap();
    fs::create_dir_all(&trip2_dir).unwrap();

    let photo1 = trip1_dir.join("DSC_0001.JPG");
    let photo2 = trip2_dir.join("DSC_0001.JPG");
    File::create(&photo1)
        .unwrap()
        .write_all(b"JPEG DSC_0001 TRIP 1")
        .unwrap();
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG DSC_0001 TRIP 2")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in hierarchy mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(
        org_output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    // 2. Run avicull correct with ambiguous query "DSC_0001"
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct");

    // Non-interactive execution must fail when matches > 1
    assert!(
        !correct_output.status.success(),
        "Ambiguous file query should fail in non-interactive mode"
    );

    let stderr = String::from_utf8_lossy(&correct_output.stderr);
    assert!(
        stderr.contains("Bharatpur/DSC_0001.JPG"),
        "stderr should list Bharatpur/DSC_0001.JPG choice, got: {}",
        stderr
    );
    assert!(
        stderr.contains("Corbett/DSC_0001.JPG"),
        "stderr should list Corbett/DSC_0001.JPG choice, got: {}",
        stderr
    );

    // 3. Disambiguating with relative path should succeed
    let disambiguated_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "Bharatpur/DSC_0001.JPG",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct with relative path");

    assert!(
        disambiguated_output.status.success(),
        "Disambiguated correct failed: stderr: {}",
        String::from_utf8_lossy(&disambiguated_output.stderr)
    );

    let db_path = input_dir.join("birds_hierarchy/.avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    let bharatpur_bundle = bundles
        .iter()
        .find(|(_, b)| b.trip_folder_name == "Bharatpur")
        .expect("Bharatpur bundle must exist");
    assert_eq!(bharatpur_bundle.0.assigned_species_slug, "Painted_Stork");
    assert!(bharatpur_bundle.0.is_user_override);

    let corbett_bundle = bundles
        .iter()
        .find(|(_, b)| b.trip_folder_name == "Corbett")
        .expect("Corbett bundle must exist");
    assert_ne!(corbett_bundle.0.assigned_species_slug, "Painted_Stork");
    assert!(!corbett_bundle.0.is_user_override);
}

#[test]
fn test_cli_correct_burst_propagation() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo1 = trip_dir.join("DSC_0001.JPG");
    let sidecar1 = trip_dir.join("DSC_0001.xmp");
    File::create(&photo1)
        .unwrap()
        .write_all(b"JPEG DSC_0001 DATA")
        .unwrap();
    File::create(&sidecar1)
        .unwrap()
        .write_all(b"<xmp>DSC_0001 SIDECAR</xmp>")
        .unwrap();

    let photo2 = trip_dir.join("DSC_0002.JPG");
    let sidecar2 = trip_dir.join("DSC_0002.xmp");
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG DSC_0002 DATA")
        .unwrap();
    File::create(&sidecar2)
        .unwrap()
        .write_all(b"<xmp>DSC_0002 SIDECAR</xmp>")
        .unwrap();

    let photo3 = trip_dir.join("DSC_0003.JPG");
    let sidecar3 = trip_dir.join("DSC_0003.xmp");
    File::create(&photo3)
        .unwrap()
        .write_all(b"JPEG DSC_0003 DATA")
        .unwrap();
    File::create(&sidecar3)
        .unwrap()
        .write_all(b"<xmp>DSC_0003 SIDECAR</xmp>")
        .unwrap();

    let ino1 = fs::metadata(&photo1).unwrap().ino();
    let ino2 = fs::metadata(&photo2).unwrap().ino();
    let ino3 = fs::metadata(&photo3).unwrap().ino();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in hierarchy mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(
        org_output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), "manifest.sqlite should exist");

    // Assign a common burst_id to all 3 frames in SQLite
    let sqlite_conn = rusqlite::Connection::open(&db_path).unwrap();
    sqlite_conn
        .execute("UPDATE bundles SET burst_id = 'burst-0001'", [])
        .unwrap();
    drop(sqlite_conn);

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles_before = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles_before.len(), 3);
    for (dec, _) in &bundles_before {
        assert_eq!(dec.burst_id.as_deref(), Some("burst-0001"));
        assert_ne!(dec.assigned_species_slug, "Painted_Stork");
        assert!(!dec.is_user_override);
    }

    // 2. Run avicull correct with --burst targeting DSC_0001
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Painted Stork",
            "--burst",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(
        correct_output.status.success(),
        "avicull correct --burst failed: stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // 3. Verify all 3 frames in SQLite have been reclassified to Painted_Stork
    let bundles_after = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles_after.len(), 3);
    for (dec, bundle) in &bundles_after {
        assert_eq!(
            dec.assigned_species_slug, "Painted_Stork",
            "Bundle {} was not reclassified",
            bundle.original_stem
        );
        assert_eq!(dec.routing_tier, RoutingTier::ConfirmedSpecies);
        assert!(
            (dec.final_confidence - 1.0).abs() < f32::EPSILON,
            "final_confidence should be 1.0"
        );
        assert!(dec.is_user_override, "is_user_override must be true");
    }

    // 4. Verify hard links on disk in Painted_Stork directory for all 3 frames
    let target_dir = output_dir.join("Painted_Stork");
    assert!(target_dir.exists(), "Target directory must exist");

    for stem in &["DSC_0001", "DSC_0002", "DSC_0003"] {
        let entries: Vec<_> = fs::read_dir(&target_dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let photo_link = entries
            .iter()
            .find(|p| {
                let name = p.file_name().unwrap().to_string_lossy();
                name.starts_with(stem) && name.ends_with(".JPG")
            })
            .unwrap_or_else(|| panic!("Hard link for {} must exist in Painted_Stork", stem));
        let sidecar_link = entries
            .iter()
            .find(|p| {
                let name = p.file_name().unwrap().to_string_lossy();
                name.starts_with(stem) && name.ends_with(".xmp")
            })
            .unwrap_or_else(|| panic!("Sidecar link for {} must exist in Painted_Stork", stem));
        assert!(photo_link.exists());
        assert!(sidecar_link.exists());
    }

    // Inode parity verification
    let entries: Vec<_> = fs::read_dir(&target_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let l1 = entries
        .iter()
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("DSC_0001")
                && p.extension().is_some_and(|e| e == "JPG")
        })
        .unwrap();
    let l2 = entries
        .iter()
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("DSC_0002")
                && p.extension().is_some_and(|e| e == "JPG")
        })
        .unwrap();
    let l3 = entries
        .iter()
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("DSC_0003")
                && p.extension().is_some_and(|e| e == "JPG")
        })
        .unwrap();

    assert_eq!(fs::metadata(l1).unwrap().ino(), ino1);
    assert_eq!(fs::metadata(l2).unwrap().ino(), ino2);
    assert_eq!(fs::metadata(l3).unwrap().ino(), ino3);

    // Strict Source Immutability
    assert_eq!(fs::metadata(&photo1).unwrap().ino(), ino1);
    assert_eq!(fs::metadata(&photo2).unwrap().ino(), ino2);
    assert_eq!(fs::metadata(&photo3).unwrap().ino(), ino3);
}

#[test]
fn test_cli_correct_burst_not_propagated_without_flag_in_non_interactive() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo1 = trip_dir.join("DSC_0001.JPG");
    let photo2 = trip_dir.join("DSC_0002.JPG");
    File::create(&photo1).unwrap().write_all(b"JPEG 1").unwrap();
    File::create(&photo2).unwrap().write_all(b"JPEG 2").unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    let sqlite_conn = rusqlite::Connection::open(&db_path).unwrap();
    sqlite_conn
        .execute("UPDATE bundles SET burst_id = 'burst-0002'", [])
        .unwrap();
    drop(sqlite_conn);

    // Call correct WITHOUT --burst in non-interactive mode
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct");
    assert!(correct_output.status.success());

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    let b1 = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0001")
        .unwrap();
    let b2 = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0002")
        .unwrap();

    assert_eq!(b1.0.assigned_species_slug, "Painted_Stork");
    assert!(b1.0.is_user_override);

    assert_ne!(b2.0.assigned_species_slug, "Painted_Stork");
    assert!(!b2.0.is_user_override);
}

#[test]
fn test_cli_correct_burst_propagation_inplace_mode() {
    let dir = tempdir().expect("create temp dir");
    let trip_dir = dir.path().join("Photos");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo1 = trip_dir.join("DSC_0001.JPG");
    let sidecar1 = trip_dir.join("DSC_0001.xmp");
    File::create(&photo1).unwrap().write_all(b"JPEG 1").unwrap();
    File::create(&sidecar1)
        .unwrap()
        .write_all(b"XMP 1")
        .unwrap();

    let photo2 = trip_dir.join("DSC_0002.JPG");
    let sidecar2 = trip_dir.join("DSC_0002.xmp");
    File::create(&photo2).unwrap().write_all(b"JPEG 2").unwrap();
    File::create(&sidecar2)
        .unwrap()
        .write_all(b"XMP 2")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            trip_dir.to_str().unwrap(),
            "--mode",
            "in-place",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    let db_path = trip_dir.join(".avicull/manifest.sqlite");
    let sqlite_conn = rusqlite::Connection::open(&db_path).unwrap();
    sqlite_conn
        .execute("UPDATE bundles SET burst_id = 'burst-inplace-01'", [])
        .unwrap();
    drop(sqlite_conn);

    // Call correct with -b (short alias for --propagate-burst) in in-place mode
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            trip_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Painted Stork",
            "-b",
        ])
        .output()
        .expect("failed to run avicull correct with -b");
    assert!(
        correct_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles.len(), 2);
    for (dec, b) in &bundles {
        assert_eq!(
            dec.assigned_species_slug, "Painted_Stork",
            "Bundle {} was not reclassified in in-place mode",
            b.original_stem
        );
        assert!(dec.is_user_override);
    }

    // Verify files on disk were renamed in-place
    let entries: Vec<_> = fs::read_dir(&trip_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let r1 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.starts_with("DSC_0001_Painted_Stork") && name.ends_with(".JPG")
    });
    let s1 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.starts_with("DSC_0001_Painted_Stork") && name.ends_with(".xmp")
    });
    let r2 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.starts_with("DSC_0002_Painted_Stork") && name.ends_with(".JPG")
    });
    let s2 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.starts_with("DSC_0002_Painted_Stork") && name.ends_with(".xmp")
    });

    assert!(
        r1.is_some(),
        "DSC_0001 JPG should be renamed with Painted_Stork"
    );
    assert!(
        s1.is_some(),
        "DSC_0001 XMP should be renamed with Painted_Stork"
    );
    assert!(
        r2.is_some(),
        "DSC_0002 JPG should be renamed with Painted_Stork"
    );
    assert!(
        s2.is_some(),
        "DSC_0002 XMP should be renamed with Painted_Stork"
    );
}
