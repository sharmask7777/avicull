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
