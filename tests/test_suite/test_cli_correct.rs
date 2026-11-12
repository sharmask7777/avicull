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
    let initial_slug = bundles[0].0.assigned_species_slug();
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
    assert_eq!(decision.assigned_species_slug(), "Painted_Stork");
    assert_eq!(decision.routing_tier, RoutingTier::ConfirmedSpecies);
    assert!(
        (decision.final_confidence - 1.0).abs() < f32::EPSILON,
        "final_confidence should be 1.0"
    );
    assert!(decision.is_user_override, "is_user_override must be true");

    // 4. Verify hardlink exists under birds_hierarchy/Bird/Painted_Stork/
    let new_species_dir = output_dir.join("Bird/Painted_Stork");
    assert!(
        new_species_dir.exists(),
        "Bird/Painted_Stork directory must exist"
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

    // 5. Verify bird_catalog.html is regenerated with new species
    let catalog_path = output_dir.join("bird_catalog.html");
    assert!(catalog_path.exists(), "bird_catalog.html should exist");
    let catalog_content = fs::read_to_string(&catalog_path).unwrap();
    assert!(
        catalog_content.contains("Painted_Stork") || catalog_content.contains("Painted Stork"),
        "bird_catalog.html should contain updated species 'Painted_Stork'"
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
        b2.0.assigned_species_slug(),
        "Black-necked_Stork",
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

    // 2. Correct to Painted Stork in in-place (no-rename) mode
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

    // 3. Verify files on disk remain strictly UNTOUCHED in in-place mode
    let entries: Vec<_> = fs::read_dir(&trip_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        entries.iter().any(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name == "DSC_0005.JPG"
        }),
        "Original untouched photo DSC_0005.JPG must exist"
    );
    assert!(
        entries.iter().any(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name == "DSC_0005.xmp"
        }),
        "Original untouched sidecar DSC_0005.xmp must exist"
    );
    assert!(
        !entries.iter().any(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.contains("Painted_Stork")
        }),
        "No renamed Painted_Stork files should exist on disk in in-place mode"
    );

    // 4. Verify SQLite record is updated with user override
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    let b5 = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0005")
        .expect("DSC_0005 must exist in manifest");
    assert_eq!(b5.0.assigned_species_slug(), "Painted_Stork");
    assert!(b5.0.is_user_override);
    assert_eq!(b5.0.routing_tier, RoutingTier::ConfirmedSpecies);
}

#[test]
fn test_cli_correct_inplace_rename_mode() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let f1 = trip_dir.join("DSC_0005.JPG");
    let s1 = trip_dir.join("DSC_0005.xmp");
    File::create(&f1).unwrap().write_all(b"JPEG DATA").unwrap();
    File::create(&s1).unwrap().write_all(b"XMP DATA").unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Initial organize in in-place-rename mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place-rename",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize in-place-rename");
    assert!(org_output.status.success());

    // 2. Correct to Painted Stork in in-place-rename mode
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place-rename",
            "--file",
            "DSC_0005",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct in-place-rename");
    assert!(
        correct_output.status.success(),
        "avicull correct in-place-rename failed: {}",
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
    assert_eq!(b5.0.assigned_species_slug(), "Painted_Stork");
    assert!(b5.0.is_user_override);
    assert_eq!(b5.0.routing_tier, RoutingTier::ConfirmedSpecies);

    // 5. Verify catalog.html regenerated by correct references the renamed path
    let catalog_html = fs::read_to_string(input_dir.join("catalog.html")).unwrap();
    assert!(
        catalog_html.contains("Painted_Stork"),
        "Regenerated catalog.html must reference renamed Painted_Stork file"
    );
    assert!(
        !catalog_html.contains("\"current_path\":\"Photos/Bharatpur/DSC_0005.JPG\""),
        "Regenerated catalog.html must not reference stale unrenamed file"
    );
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
    assert_eq!(bharatpur_bundle.0.assigned_species_slug(), "Painted_Stork");
    assert!(bharatpur_bundle.0.is_user_override);

    let corbett_bundle = bundles
        .iter()
        .find(|(_, b)| b.trip_folder_name == "Corbett")
        .expect("Corbett bundle must exist");
    assert_ne!(corbett_bundle.0.assigned_species_slug(), "Painted_Stork");
    assert!(!corbett_bundle.0.is_user_override);
}

#[test]
fn test_cli_correct_duplicate_stem_across_subdirectories_in_place_mode() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("pictures_archive");
    let trip1_dir = input_dir.join("Dubare - 2 and nagarhole 2024 /Processed - compressed");
    let trip2_dir = input_dir.join("Kashmir and Dubare -1  2024/Processed/Dubare compressed");
    fs::create_dir_all(&trip1_dir).unwrap();
    fs::create_dir_all(&trip2_dir).unwrap();

    let photo1 = trip1_dir.join("dubare_19.jpg");
    let photo2 = trip2_dir.join("dubare_19.jpg");
    File::create(&photo1)
        .unwrap()
        .write_all(b"JPEG DUBARE 19 TRIP 1")
        .unwrap();
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG DUBARE 19 TRIP 2")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in default in-place mode
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
        .expect("failed to run avicull organize");
    assert!(
        org_output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    // 2. Querying ambiguous bare stem "dubare_19" fails non-interactively
    let ambiguous_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "dubare_19",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct with bare stem");

    assert!(
        !ambiguous_output.status.success(),
        "Ambiguous file query should fail in non-interactive mode"
    );
    let stderr = String::from_utf8_lossy(&ambiguous_output.stderr);
    assert!(
        stderr.contains("Dubare - 2 and nagarhole 2024 /Processed - compressed/dubare_19.jpg"),
        "stderr should list choice 1, got: {stderr}"
    );
    assert!(
        stderr.contains("Kashmir and Dubare -1  2024/Processed/Dubare compressed/dubare_19.jpg"),
        "stderr should list choice 2, got: {stderr}"
    );

    // 3. Querying with exact relative path succeeds
    let disambiguated_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "Dubare - 2 and nagarhole 2024 /Processed - compressed/dubare_19.jpg",
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

    // 4. Verify manifest overrides only target photo1
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();

    let b1 = bundles
        .iter()
        .find(|(_, b)| {
            b.relative_path.as_ref().is_some_and(|p| {
                p == Path::new(
                    "Dubare - 2 and nagarhole 2024 /Processed - compressed/dubare_19.jpg",
                )
            })
        })
        .expect("Bundle 1 must exist");
    assert_eq!(b1.0.assigned_species_slug(), "Painted_Stork");
    assert!(b1.0.is_user_override);

    let b2 = bundles
        .iter()
        .find(|(_, b)| {
            b.relative_path.as_ref().is_some_and(|p| {
                p == Path::new(
                    "Kashmir and Dubare -1  2024/Processed/Dubare compressed/dubare_19.jpg",
                )
            })
        })
        .expect("Bundle 2 must exist");
    assert_ne!(b2.0.assigned_species_slug(), "Painted_Stork");
    assert!(!b2.0.is_user_override);
}

#[test]
fn test_cli_correct_does_not_falsely_match_prefix_stem_photo() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Kashmir and Dubare/Processed/Dubare");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo_base = trip_dir.join("dubare.jpg");
    let photo_num = trip_dir.join("dubare_19.jpg");
    File::create(&photo_base)
        .unwrap()
        .write_all(b"JPEG DUBARE BASE")
        .unwrap();
    File::create(&photo_num)
        .unwrap()
        .write_all(b"JPEG DUBARE NUM 19")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in-place
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
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    // 2. Correct dubare_19.jpg using its relative path. It must NOT match dubare.jpg!
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "Kashmir and Dubare/Processed/Dubare/dubare_19.jpg",
            "--species",
            "Indian Roller",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(
        correct_output.status.success(),
        "Correction failed with stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // Verify only dubare_19 was updated
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();

    let b_base = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "dubare")
        .expect("dubare must exist");
    assert_ne!(b_base.0.assigned_species_slug(), "Indian_Roller");

    let b_num = bundles
        .iter()
        .find(|(_, b)| b.original_stem == "dubare_19")
        .expect("dubare_19 must exist");
    assert_eq!(b_num.0.assigned_species_slug(), "Indian_Roller");
    assert!(b_num.0.is_user_override);
}

#[test]
fn test_cli_correct_root_file_disambiguation_with_dot_slash() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let root_photo = input_dir.join("DSC_0001.JPG");
    let sub_photo = trip_dir.join("DSC_0001.JPG");
    File::create(&root_photo)
        .unwrap()
        .write_all(b"JPEG ROOT PHOTO")
        .unwrap();
    File::create(&sub_photo)
        .unwrap()
        .write_all(b"JPEG SUB PHOTO")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in-place
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
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    // 2. Query with bare stem fails with ambiguity
    let ambiguous_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
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
        .expect("failed to run avicull correct with bare stem");
    assert!(!ambiguous_output.status.success());
    let stderr = String::from_utf8_lossy(&ambiguous_output.stderr);
    assert!(stderr.contains("Multiple photo bundles matched query 'DSC_0001'"));

    // 3. Query with explicit root prefix "./DSC_0001.JPG" succeeds for root only
    let root_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "./DSC_0001.JPG",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct with ./ prefix");
    assert!(
        root_output.status.success(),
        "Query with ./DSC_0001.JPG must succeed for root photo: {}",
        String::from_utf8_lossy(&root_output.stderr)
    );

    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();

    let root_bundle = bundles
        .iter()
        .find(|(_, b)| {
            b.relative_path
                .as_ref()
                .is_some_and(|p| p.parent().is_none_or(|par| par.as_os_str().is_empty()))
        })
        .expect("Root bundle must exist");
    assert_eq!(root_bundle.0.assigned_species_slug(), "Painted_Stork");
    assert!(root_bundle.0.is_user_override);

    let sub_bundle = bundles
        .iter()
        .find(|(_, b)| b.trip_folder_name == "Bharatpur")
        .expect("Subfolder bundle must exist");
    assert_ne!(sub_bundle.0.assigned_species_slug(), "Painted_Stork");
    assert!(!sub_bundle.0.is_user_override);
}

#[test]
fn test_cli_correct_renamed_file_does_not_match_different_stem_in_same_directory() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let db_dir = input_dir.join(".avicull");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("manifest.sqlite");

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    db.record_run("run_test", "/fake/input", "/fake/output", false, "{}")
        .unwrap();

    let dummy_decision = avicull::types::BundleDecision {
        bundle_id: "bundle_dsc_0001".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        classification: avicull::types::Classification::from_slug("House_Sparrow"),
        frame_confidence: 0.95,
        final_confidence: 0.95,
        routing_tier: avicull::types::RoutingTier::ConfirmedSpecies,
        top_predictions: vec![],
        is_user_override: false,
    };

    // Record a bundle whose file has been renamed on disk with date and location prefix
    db.record_bundle(
        "run_test",
        &avicull::types::AssetBundle {
            bundle_id: "bundle_dsc_0001".to_string(),
            source_inode: 9001,
            trip_folder_name: "Bharatpur".to_string(),
            original_stem: "DSC_0001".to_string(),
            primary_image: std::path::PathBuf::from("Bharatpur/2024-05-01_Bharatpur_DSC_0001.JPG"),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: Some(std::path::PathBuf::from(
                "Bharatpur/2024-05-01_Bharatpur_DSC_0001.JPG",
            )),
        },
        &dummy_decision,
    )
    .unwrap();
    drop(db);

    // Querying for a DIFFERENT stem in the same folder: "Bharatpur/DSC_0002.JPG"
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "Bharatpur/DSC_0002.JPG",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(
        !correct_output.status.success(),
        "Query for nonexistent stem DSC_0002 must fail"
    );
    let stderr = String::from_utf8_lossy(&correct_output.stderr);
    assert!(
        stderr.contains("No photo bundle matching 'Bharatpur/DSC_0002.JPG' was found in manifest"),
        "stderr should say not found, but got: {stderr}"
    );
}

#[test]
fn test_cli_correct_nonexistent_file_in_renamed_subdirectory_returns_not_found() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("IMG_0001.JPG");
    let photo2 = trip_dir.join("IMG_0002.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DATA 1")
        .unwrap();
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG DATA 2")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // Organize in in-place-rename mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--mode",
            "in-place-rename",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("failed to run avicull organize");
    assert!(org_output.status.success());

    // Re-run organize to scan the already-renamed files on disk
    let org_output2 = Command::new(env!("CARGO_BIN_EXE_avicull"))
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
        .expect("failed to run avicull organize 2");
    assert!(org_output2.status.success());

    // Query a nonexistent file in that same subdirectory
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "Bharatpur/nonexistent_photo.jpg",
            "--species",
            "Painted Stork",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(!correct_output.status.success());
    let stderr = String::from_utf8_lossy(&correct_output.stderr);
    assert!(
        stderr.contains("No photo bundle matching 'Bharatpur/nonexistent_photo.jpg' was found"),
        "Expected not found error, got: {stderr}"
    );
    assert!(
        !stderr.contains("Multiple photo bundles matched"),
        "Must not falsely report multiple matched bundles for nonexistent file: {stderr}"
    );
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
        assert_ne!(dec.assigned_species_slug(), "Painted_Stork");
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
            dec.assigned_species_slug(),
            "Painted_Stork",
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
    let target_dir = output_dir.join("Bird/Painted_Stork");
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

    assert_eq!(b1.0.assigned_species_slug(), "Painted_Stork");
    assert!(b1.0.is_user_override);

    assert_ne!(b2.0.assigned_species_slug(), "Painted_Stork");
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
            "in-place-rename",
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
            dec.assigned_species_slug(),
            "Painted_Stork",
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
        name.contains("DSC_0001") && name.contains("Painted_Stork") && name.ends_with(".JPG")
    });
    let s1 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.contains("DSC_0001") && name.contains("Painted_Stork") && name.ends_with(".xmp")
    });
    let r2 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.contains("DSC_0002") && name.contains("Painted_Stork") && name.ends_with(".JPG")
    });
    let s2 = entries.iter().find(|p| {
        let name = p.file_name().unwrap().to_string_lossy();
        name.contains("DSC_0002") && name.contains("Painted_Stork") && name.ends_with(".xmp")
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

#[test]
fn test_cli_correct_fuzzy_species_matching() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0001.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_0001 DATA")
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
    assert!(org_output.status.success());

    let output_dir = input_dir.join("birds_hierarchy");
    let db_path = output_dir.join(".avicull/manifest.sqlite");

    // 2. Interactive correction with typo "Panted Stork" confirming 'y'
    // Resolves to canonical "Painted_Stork"
    let mut child = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Panted Stork",
        ])
        .env("AVICULL_FORCE_INTERACTIVE", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn avicull correct");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"y\n").expect("write to stdin");
    }
    let output = child.wait_with_output().expect("wait for child");
    assert!(
        output.status.success(),
        "Interactive correct failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout_str.contains("Did you mean 'Painted Stork'?"),
        "stdout should prompt with fuzzy suggestion, got: {}",
        stdout_str
    );

    // Verify bundle was reclassified to canonical Painted_Stork
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].0.assigned_species_slug(), "Painted_Stork");
    assert!(bundles[0].0.is_user_override);

    // 3. Non-interactive correction with typo "Mallrd"
    // Logs informative suggestion while allowing custom species slug
    let non_interactive_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0001",
            "--species",
            "Mallrd",
        ])
        .output()
        .expect("failed to run avicull correct non-interactively");
    assert!(non_interactive_output.status.success());

    let non_interactive_out = format!(
        "{}\n{}",
        String::from_utf8_lossy(&non_interactive_output.stdout),
        String::from_utf8_lossy(&non_interactive_output.stderr)
    );
    assert!(
        non_interactive_out.contains("Did you mean 'Mallard'?"),
        "non-interactive output should log typo suggestion, got: {}",
        non_interactive_out
    );

    let updated_bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(updated_bundles[0].0.assigned_species_slug(), "Mallrd");
    assert!(updated_bundles[0].0.is_user_override);
}

#[test]
fn test_cli_correct_evicts_renamed_file_if_dataless() {
    let dir = tempdir().expect("create temp dir");
    // Place inside CloudStorage-like path so detect_storage_backend selects MacOSCloudStorageBackend
    let cloud_dir = dir
        .path()
        .join("Library/CloudStorage/OneDrive-Personal/Photos");
    let trip_dir = cloud_dir.join("Corbett");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0099.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG DSC_0099 DATA")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in in-place mode
    let org_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            cloud_dir.to_str().unwrap(),
            "--mode",
            "in-place-rename",
            "--models-dir",
            fixture_models_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("organize in-place");
    assert!(org_output.status.success());

    // 2. Correct to Great Hornbill
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            cloud_dir.to_str().unwrap(),
            "--mode",
            "in-place-rename",
            "--file",
            "DSC_0099",
            "--species",
            "Great Hornbill",
        ])
        .output()
        .expect("correct in-place");
    assert!(correct_output.status.success());
}

#[test]
fn test_cli_correct_no_bird_detected_variants() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Bharatpur");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo1 = trip_dir.join("DSC_0010.JPG");
    let photo2 = trip_dir.join("DSC_0020.JPG");
    File::create(&photo1)
        .unwrap()
        .write_all(b"JPEG 10")
        .unwrap();
    File::create(&photo2)
        .unwrap()
        .write_all(b"JPEG 20")
        .unwrap();

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
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    // 1. Correct DSC_0010 with --species "No Bird Detected"
    let correct1 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0010",
            "--species",
            "No Bird Detected",
        ])
        .output()
        .expect("failed to run avicull correct 1");
    assert!(
        correct1.status.success(),
        "avicull correct 'No Bird Detected' failed: {}",
        String::from_utf8_lossy(&correct1.stderr)
    );

    // Verify manifest for DSC_0010
    let bundles1 = db.get_all_bundles(None).unwrap();
    let b10 = bundles1
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0010")
        .expect("DSC_0010 bundle");
    assert_eq!(b10.0.routing_tier, RoutingTier::NoBirdDetected);
    assert_eq!(b10.0.assigned_species_slug(), "_No_Bird_Detected");
    let no_bird_dir = output_dir.join("_No_Bird_Detected");
    assert!(no_bird_dir.exists());
    let entries: Vec<_> = fs::read_dir(&no_bird_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        entries.iter().any(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("DSC_0010") && n.ends_with(".JPG")
        }),
        "Hardlink for DSC_0010 must exist in _No_Bird_Detected directory"
    );

    // 2. Correct DSC_0020 with --species "No_Bird_Detected"
    let correct2 = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0020",
            "--species",
            "No_Bird_Detected",
        ])
        .output()
        .expect("failed to run avicull correct 2");
    assert!(
        correct2.status.success(),
        "avicull correct 'No_Bird_Detected' failed: {}",
        String::from_utf8_lossy(&correct2.stderr)
    );

    // Verify manifest for DSC_0020
    let bundles2 = db.get_all_bundles(None).unwrap();
    let b20 = bundles2
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0020")
        .expect("DSC_0020 bundle");
    assert_eq!(b20.0.routing_tier, RoutingTier::NoBirdDetected);
    assert_eq!(b20.0.assigned_species_slug(), "_No_Bird_Detected");
    let entries2: Vec<_> = fs::read_dir(&no_bird_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        entries2.iter().any(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("DSC_0020") && n.ends_with(".JPG")
        }),
        "Hardlink for DSC_0020 must exist in _No_Bird_Detected directory"
    );

    // Verify manifest for DSC_0020
    let bundles2 = db.get_all_bundles(None).unwrap();
    let b20 = bundles2
        .iter()
        .find(|(_, b)| b.original_stem == "DSC_0020")
        .expect("DSC_0020 bundle");
    assert_eq!(b20.0.routing_tier, RoutingTier::NoBirdDetected);
    assert_eq!(b20.0.assigned_species_slug(), "_No_Bird_Detected");
}

#[test]
fn test_cli_correct_subcommand_overrides_category_and_species_simultaneously() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("WesternGhats");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("DSC_0050.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"MAMMAL SQUIRREL DATA")
        .unwrap();

    let src_meta = fs::metadata(&photo).unwrap();
    let src_ino = src_meta.ino();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Organize in hierarchy mode initially
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
        .expect("run organize");
    assert!(
        org_output.status.success(),
        "organize failed: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    let output_dir = input_dir.join("birds_hierarchy");

    // 2. Call avicull correct with --category Mammalia --species "Malabar Giant Squirrel"
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "DSC_0050",
            "--category",
            "Mammalia",
            "--species",
            "Malabar Giant Squirrel",
        ])
        .output()
        .expect("run avicull correct with --category");

    assert!(
        correct_output.status.success(),
        "avicull correct failed: stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // 3. Verify SQLite manifest has category = 'Mammalia', assigned_species_slug = 'Malabar_Giant_Squirrel'
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let bundles = db.get_all_bundles(None).unwrap();
    assert_eq!(bundles.len(), 1);
    let (decision, _bundle) = &bundles[0];
    assert_eq!(decision.assigned_species_slug(), "Malabar_Giant_Squirrel");
    assert!(decision.is_user_override);

    // 4. Verify hardlink exists under birds_hierarchy/Mammalia/Malabar_Giant_Squirrel/
    let mammal_dir = output_dir.join("Mammalia/Malabar_Giant_Squirrel");
    assert!(
        mammal_dir.exists(),
        "Mammalia/Malabar_Giant_Squirrel directory must exist: {:?}",
        output_dir
    );
    let entries: Vec<_> = fs::read_dir(&mammal_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let photo_link = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("DSC_0050") && name.ends_with(".JPG")
        })
        .expect("Photo hardlink must exist under Mammalia/Malabar_Giant_Squirrel/");

    let dst_meta = fs::metadata(photo_link).unwrap();
    assert_eq!(
        dst_meta.ino(),
        src_ino,
        "Hardlink inode must match source inode"
    );
    assert!(dst_meta.nlink() >= 2, "nlink must be >= 2");
}

#[test]
fn test_cli_correct_without_input_flag_resolves_root_not_avicull_dir() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Photos");
    let trip_dir = input_dir.join("Hesaraghatta");
    fs::create_dir_all(&trip_dir).unwrap();

    let photo = trip_dir.join("IMG_0100.JPG");
    File::create(&photo)
        .unwrap()
        .write_all(b"JPEG IMG_0100 TEST DATA")
        .unwrap();

    let fixture_models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");

    // 1. Run avicull organize in in-place mode
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
        .expect("failed to run avicull organize");

    assert!(
        org_output.status.success(),
        "organize failed: stderr: {}",
        String::from_utf8_lossy(&org_output.stderr)
    );

    let catalog_path = input_dir.join("catalog.html");
    assert!(
        catalog_path.exists(),
        "catalog.html should exist in input_dir"
    );
    let _initial_catalog_modtime = fs::metadata(&catalog_path).unwrap().modified().unwrap();

    // 2. Call avicull correct with current_dir = input_dir and WITHOUT --input
    let correct_output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .current_dir(&input_dir)
        .args([
            "correct",
            "--category",
            "Bird",
            "--species",
            "Indian_Roller",
            "--file",
            "IMG_0100",
        ])
        .output()
        .expect("failed to run avicull correct");

    assert!(
        correct_output.status.success(),
        "avicull correct failed: stderr: {}",
        String::from_utf8_lossy(&correct_output.stderr)
    );

    // 3. Crucial Assertions:
    // (a) `<input_dir>/.avicull/catalog.html` must NEVER be created!
    let wrong_catalog = input_dir.join(".avicull/catalog.html");
    assert!(
        !wrong_catalog.exists(),
        "catalog.html should NEVER be created inside .avicull/"
    );

    // (b) `<input_dir>/.avicull/.avicull` must NEVER be created!
    let wrong_nested_avicull = input_dir.join(".avicull/.avicull");
    assert!(
        !wrong_nested_avicull.exists(),
        "Nested .avicull/.avicull directory should NEVER be created"
    );

    // (c) `<input_dir>/catalog.html` must be updated
    let updated_catalog_content = fs::read_to_string(&catalog_path).unwrap();
    assert!(
        updated_catalog_content.contains("Indian Roller"),
        "catalog.html in input_dir should contain updated species 'Indian Roller'"
    );
}
