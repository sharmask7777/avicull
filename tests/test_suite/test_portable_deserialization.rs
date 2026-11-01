use avicull::db::ManifestDb;
use avicull::scanner::scan_input_tree;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

#[test]
fn test_scanner_populates_relative_path() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("photos");
    let trip_dir = input.join("2025-11_Bharatpur");
    let nested_dir = input.join("2026-01_Ranganathittu/Morning");
    fs::create_dir_all(&trip_dir).unwrap();
    fs::create_dir_all(&nested_dir).unwrap();

    let img1 = trip_dir.join("DSC_0001.JPG");
    let img2 = nested_dir.join("IMG_0002.JPG");
    File::create(&img1).unwrap().write_all(b"image 1").unwrap();
    File::create(&img2).unwrap().write_all(b"image 2").unwrap();

    let bundles = scan_input_tree(&input).expect("scan_input_tree failed");
    assert_eq!(bundles.len(), 2);

    let b1 = bundles
        .iter()
        .find(|b| b.original_stem == "DSC_0001")
        .expect("DSC_0001 bundle missing");
    assert_eq!(
        b1.relative_path,
        Some(PathBuf::from("2025-11_Bharatpur/DSC_0001.JPG")),
        "relative_path must be populated relative to input_dir"
    );

    let b2 = bundles
        .iter()
        .find(|b| b.original_stem == "IMG_0002")
        .expect("IMG_0002 bundle missing");
    assert_eq!(
        b2.relative_path,
        Some(PathBuf::from("2026-01_Ranganathittu/Morning/IMG_0002.JPG")),
        "relative_path must support nested folders"
    );
}

#[test]
fn test_cross_machine_portable_deserialization() {
    let tmp = tempfile::tempdir().unwrap();
    let machine_a_root = tmp.path().join("machine_a");
    let machine_b_root = tmp.path().join("machine_b");

    let input_a = machine_a_root.join("photos");
    let trip1_a = input_a.join("2025-11_Bharatpur");
    let trip2_a = input_a.join("2026-01_Ranganathittu");
    fs::create_dir_all(&trip1_a).unwrap();
    fs::create_dir_all(&trip2_a).unwrap();

    let img1_a = trip1_a.join("DSC_0001.JPG");
    let xmp1_a = trip1_a.join("DSC_0001.xmp");
    let img2_a = trip2_a.join("IMG_0002.JPG");

    File::create(&img1_a)
        .unwrap()
        .write_all(b"image 1 content")
        .unwrap();
    File::create(&xmp1_a)
        .unwrap()
        .write_all(b"xmp 1 content")
        .unwrap();
    File::create(&img2_a)
        .unwrap()
        .write_all(b"image 2 content")
        .unwrap();

    let bin_path = env!("CARGO_BIN_EXE_avicull");
    let fixture_models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");
    let output_a = machine_a_root.join("birds_hierarchy");

    // Run organize on machine_a
    let output_res = Command::new(bin_path)
        .args([
            "organize",
            "--mode",
            "hierarchy",
            "--input",
            input_a.to_str().unwrap(),
            "--output",
            output_a.to_str().unwrap(),
            "--models-dir",
            fixture_models.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run organize on machine_a");

    assert!(
        output_res.status.success(),
        "organize on machine_a failed: {}",
        String::from_utf8_lossy(&output_res.stderr)
    );

    let db_path_a = output_a.join(".avicull/manifest.sqlite");
    assert!(
        db_path_a.exists(),
        "manifest.sqlite should exist on machine_a"
    );

    // Copy photos and organized output to machine_b with completely different root paths
    let input_b = machine_b_root.join("photos");
    let output_b = machine_b_root.join("birds_hierarchy");

    copy_dir_all(&input_a, &input_b).expect("copy input to machine_b");
    copy_dir_all(&output_a, &output_b).expect("copy output to machine_b");

    let img1_b = input_b.join("2025-11_Bharatpur/DSC_0001.JPG");
    let meta_a = fs::metadata(&img1_a).unwrap();
    let meta_b = fs::metadata(&img1_b).unwrap();
    assert_ne!(
        meta_a.ino(),
        meta_b.ino(),
        "Copied files must have different inodes on machine_b"
    );

    // Verify via db API on machine_b that ExistingBundleMap resolves bundle by relative_path
    let db_b = ManifestDb::open(&output_b.join(".avicull/manifest.sqlite")).unwrap();
    let existing_map = db_b.get_existing_bundle_map().unwrap();
    let bundles_b = scan_input_tree(&input_b).unwrap();
    assert_eq!(bundles_b.len(), 2);

    for b in &bundles_b {
        assert!(
            b.relative_path.is_some(),
            "Bundle scanned on machine_b must have relative_path populated"
        );
        let found = existing_map.get_by_bundle(b);
        assert!(
            found.is_some(),
            "ExistingBundleMap must find bundle {:?} on machine_b",
            b.relative_path
        );
    }

    // Run avicull organize on machine_b without AI models
    let run_b = Command::new(bin_path)
        .args([
            "organize",
            "--mode",
            "hierarchy",
            "--input",
            input_b.to_str().unwrap(),
            "--output",
            output_b.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run organize on machine_b");

    assert!(
        run_b.status.success(),
        "organize on machine_b should succeed without models: {}",
        String::from_utf8_lossy(&run_b.stderr)
    );

    let stdout_b = String::from_utf8_lossy(&run_b.stdout);
    assert!(
        stdout_b.contains("already up to date") || stdout_b.contains("0 new bundles detected"),
        "machine_b must achieve 100% cache hit without re-running inference, stdout: {}",
        stdout_b
    );
    assert!(
        !stdout_b.contains("Running detection and classification"),
        "Inference should not be re-run on machine_b"
    );
}

#[test]
fn test_nested_subfolders_same_stem_portable_deserialization() {
    let tmp = tempfile::tempdir().unwrap();
    let machine_a_root = tmp.path().join("machine_a");
    let machine_b_root = tmp.path().join("machine_b");

    let input_a = machine_a_root.join("photos");
    let morning_a = input_a.join("trip1/morning");
    let evening_a = input_a.join("trip1/evening");
    fs::create_dir_all(&morning_a).unwrap();
    fs::create_dir_all(&evening_a).unwrap();

    let img_morning_a = morning_a.join("DSC_0001.JPG");
    let img_evening_a = evening_a.join("DSC_0001.JPG");

    File::create(&img_morning_a)
        .unwrap()
        .write_all(b"morning bird")
        .unwrap();
    File::create(&img_evening_a)
        .unwrap()
        .write_all(b"evening bird")
        .unwrap();

    let bin_path = env!("CARGO_BIN_EXE_avicull");
    let fixture_models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models");
    let output_a = machine_a_root.join("birds_hierarchy");

    // Organize on machine_a
    let output_res = Command::new(bin_path)
        .args([
            "organize",
            "--mode",
            "hierarchy",
            "--input",
            input_a.to_str().unwrap(),
            "--output",
            output_a.to_str().unwrap(),
            "--models-dir",
            fixture_models.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run organize on machine_a");

    assert!(output_res.status.success());

    // Copy to machine_b
    let input_b = machine_b_root.join("photos");
    let output_b = machine_b_root.join("birds_hierarchy");
    copy_dir_all(&input_a, &input_b).unwrap();
    copy_dir_all(&output_a, &output_b).unwrap();

    // Check DB and bundles directly
    let db_b = ManifestDb::open(&output_b.join(".avicull/manifest.sqlite")).unwrap();
    let existing_map = db_b.get_existing_bundle_map().unwrap();
    let bundles_b = scan_input_tree(&input_b).unwrap();
    assert_eq!(bundles_b.len(), 2);

    let morning_bundle = bundles_b
        .iter()
        .find(|b| b.relative_path == Some(PathBuf::from("trip1/morning/DSC_0001.JPG")))
        .expect("morning bundle must be found by relative_path");
    let evening_bundle = bundles_b
        .iter()
        .find(|b| b.relative_path == Some(PathBuf::from("trip1/evening/DSC_0001.JPG")))
        .expect("evening bundle must be found by relative_path");

    assert!(existing_map.get_by_bundle(morning_bundle).is_some());
    assert!(existing_map.get_by_bundle(evening_bundle).is_some());

    // Organize on machine_b without models
    let run_b = Command::new(bin_path)
        .args([
            "organize",
            "--mode",
            "hierarchy",
            "--input",
            input_b.to_str().unwrap(),
            "--output",
            output_b.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run organize on machine_b");

    assert!(run_b.status.success());
    let stdout_b = String::from_utf8_lossy(&run_b.stdout);
    assert!(
        stdout_b.contains("already up to date") || stdout_b.contains("0 new bundles detected"),
        "machine_b must hit cache 100% despite duplicate stems across subfolders, stdout: {}",
        stdout_b
    );
}
