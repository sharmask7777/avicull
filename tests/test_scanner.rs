use avicull::scanner::*;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_validate_volume_and_disjointness_same_dir_or_input_inside_output_rejected() {
    let dir = tempdir().expect("create temp dir");
    let output = dir.path().join("output_root");
    let input = output.join("trips"); // input nested inside output!

    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();

    let res = validate_volume_and_disjointness(&input, &output);
    assert!(res.is_err(), "Input nested inside output must fail");

    // Also identical input and output must fail
    let res_same = validate_volume_and_disjointness(&input, &input);
    assert!(res_same.is_err(), "Identical input and output must fail");
}

#[test]
fn test_validate_volume_and_disjointness_nested_output_subdir_allowed() {
    let dir = tempdir().expect("create temp dir");
    let input = dir.path().join("trips");
    let output = input.join("birds_hierarchy"); // output subfolder inside input

    fs::create_dir_all(&input).unwrap();
    let res = validate_volume_and_disjointness(&input, &output);
    assert!(res.is_ok(), "Subdirectory output inside input must succeed");
}

#[test]
fn test_validate_volume_and_disjointness_same_volume_ok() {
    let dir = tempdir().expect("create temp dir");
    let input = dir.path().join("trips");
    let output = dir.path().join("by_species");

    fs::create_dir_all(&input).unwrap();
    // output created automatically if not existing
    let res = validate_volume_and_disjointness(&input, &output);
    assert!(res.is_ok(), "Disjoint paths on same volume must succeed");
}

#[test]
fn test_scanner_sidecar_bundling() {
    let dir = tempdir().expect("create temp dir");
    let trips = dir.path().join("trips");
    let trip1 = trips.join("2025-11_Bharatpur");
    let trip2 = trips.join("2026-01_Ranganathittu");
    fs::create_dir_all(&trip1).unwrap();
    fs::create_dir_all(&trip2).unwrap();

    // In trip1:
    // DSC_0001.JPG and DSC_0001.xmp
    File::create(trip1.join("DSC_0001.JPG"))
        .unwrap()
        .write_all(b"jpeg content")
        .unwrap();
    File::create(trip1.join("DSC_0001.xmp"))
        .unwrap()
        .write_all(b"xmp content")
        .unwrap();

    // DSC_0002 has RAW (primary), JPG (companion), and ARW.xmp (sidecar)
    File::create(trip1.join("DSC_0002.ARW"))
        .unwrap()
        .write_all(b"arw content")
        .unwrap();
    File::create(trip1.join("DSC_0002.JPG"))
        .unwrap()
        .write_all(b"jpg content")
        .unwrap();
    File::create(trip1.join("DSC_0002.ARW.xmp"))
        .unwrap()
        .write_all(b"arw xmp")
        .unwrap();

    // In trip2:
    // IMG_9001.JPG and IMG_9001.XMP (upper-case sidecar)
    File::create(trip2.join("IMG_9001.JPG"))
        .unwrap()
        .write_all(b"jpeg content 9001")
        .unwrap();
    File::create(trip2.join("IMG_9001.XMP"))
        .unwrap()
        .write_all(b"xmp content 9001")
        .unwrap();

    let bundles = scan_input_tree(&trips).expect("scan_input_tree failed");
    assert_eq!(bundles.len(), 3, "Expected 3 bundles total");

    // Find DSC_0002 bundle
    let b2 = bundles
        .iter()
        .find(|b| b.original_stem.to_uppercase() == "DSC_0002")
        .expect("DSC_0002 bundle not found");

    assert_eq!(b2.primary_image.file_name().unwrap(), "DSC_0002.ARW");
    assert_eq!(b2.companion_images.len(), 1);
    assert_eq!(b2.companion_images[0].file_name().unwrap(), "DSC_0002.JPG");
    assert_eq!(b2.sidecar_files.len(), 1);
    assert_eq!(b2.sidecar_files[0].file_name().unwrap(), "DSC_0002.ARW.xmp");

    // Find IMG_9001 bundle
    let b3 = bundles
        .iter()
        .find(|b| b.original_stem.to_uppercase() == "IMG_9001")
        .expect("IMG_9001 bundle not found");
    assert_eq!(b3.primary_image.file_name().unwrap(), "IMG_9001.JPG");
    assert_eq!(b3.sidecar_files.len(), 1);
    assert_eq!(b3.sidecar_files[0].file_name().unwrap(), "IMG_9001.XMP");
}

#[test]
fn test_extract_embedded_jpeg_preview() {
    let mut fake_raw = Vec::new();
    fake_raw.extend_from_slice(b"SOME RAW HEADER BYTES");
    // Insert smaller embedded JPEG
    fake_raw.extend_from_slice(&[0xFF, 0xD8, 0x01, 0x02, 0xFF, 0xD9]);
    fake_raw.extend_from_slice(b"INTERMEDIATE BYTES");
    // Insert larger embedded preview JPEG
    let mut large_jpeg = vec![0xFF, 0xD8];
    large_jpeg.extend_from_slice(&[0x42; 256]);
    large_jpeg.extend_from_slice(&[0xFF, 0xD9]);
    fake_raw.extend_from_slice(&large_jpeg);
    fake_raw.extend_from_slice(b"TRAILING RAW BYTES");

    let extracted = extract_embedded_jpeg_preview(&fake_raw).expect("Failed to extract preview");
    assert_eq!(extracted, large_jpeg);
}
