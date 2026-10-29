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

struct PermissionGuard<'a>(&'a std::path::Path);
impl<'a> Drop for PermissionGuard<'a> {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn test_validate_volume_and_disjointness_unreadable_input_fails_with_clear_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().expect("create temp dir");
    let unreadable_input = dir.path().join("unreadable_trips");
    let output = dir.path().join("by_species");
    fs::create_dir_all(&unreadable_input).unwrap();

    let _guard = PermissionGuard(&unreadable_input);
    // Remove read permissions
    fs::set_permissions(&unreadable_input, fs::Permissions::from_mode(0o000)).unwrap();

    let res = validate_volume_and_disjointness(&unreadable_input, &output);

    assert!(
        res.is_err(),
        "Unreadable input directory must fail preflight validation"
    );
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Permission denied")
            || err_msg.contains("unreadable")
            || err_msg.contains("Cannot read"),
        "Error message should explain permission denial, got: {}",
        err_msg
    );
}

#[test]
fn test_validate_volume_and_disjointness_cloudstorage_tcc_message() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().expect("create temp dir");
    let cloud_input = dir.path().join("Library/CloudStorage/MockDrive-test");
    let output = dir.path().join("by_species");
    fs::create_dir_all(&cloud_input).unwrap();

    let _guard = PermissionGuard(&cloud_input);
    // Remove read permissions to simulate TCC restriction
    fs::set_permissions(&cloud_input, fs::Permissions::from_mode(0o000)).unwrap();

    let res = validate_volume_and_disjointness(&cloud_input, &output);

    assert!(res.is_err(), "Unreadable CloudStorage input must fail");
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Full Disk Access") && err_msg.contains("macOS Privacy & Security"),
        "Error message must provide actionable Full Disk Access instructions, got: {}",
        err_msg
    );
}

#[test]
fn test_scanner_skips_unreadable_subdirectories_gracefully() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().expect("create temp dir");
    let trips = dir.path().join("trips");
    let valid_trip = trips.join("valid_trip");
    let locked_subfolder = trips.join("locked_subfolder");

    fs::create_dir_all(&valid_trip).unwrap();
    fs::create_dir_all(&locked_subfolder).unwrap();

    File::create(valid_trip.join("IMG_0001.JPG"))
        .unwrap()
        .write_all(b"valid photo content")
        .unwrap();

    File::create(locked_subfolder.join("LOCKED_0001.JPG"))
        .unwrap()
        .write_all(b"locked photo content")
        .unwrap();

    let _guard = PermissionGuard(&locked_subfolder);
    // Lock the subfolder so read_dir fails inside it
    fs::set_permissions(&locked_subfolder, fs::Permissions::from_mode(0o000)).unwrap();

    let bundles_res = scan_input_tree_with_skip(&trips, None);

    assert!(
        bundles_res.is_ok(),
        "Scanner must not crash on isolated unreadable subfolders; got error: {:?}",
        bundles_res.err()
    );
    let bundles = bundles_res.unwrap();
    assert_eq!(
        bundles.len(),
        1,
        "Expected 1 valid bundle scanned from valid_trip"
    );
    assert_eq!(bundles[0].original_stem, "IMG_0001");
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
    // Insert smaller embedded JPEG with valid SOF0 marker (0xFF, 0xC0)
    fake_raw.extend_from_slice(&[
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x10, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00,
        0xFF, 0xD9,
    ]);
    fake_raw.extend_from_slice(b"INTERMEDIATE BYTES");
    // Insert larger embedded preview JPEG with valid SOF0 marker
    let mut large_jpeg = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x20, 0x00, 0x20, 0x01, 0x01, 0x11, 0x00,
    ];
    large_jpeg.extend_from_slice(&[0x42; 256]);
    large_jpeg.extend_from_slice(&[0xFF, 0xD9]);
    fake_raw.extend_from_slice(&large_jpeg);
    fake_raw.extend_from_slice(b"TRAILING RAW BYTES");

    let extracted = extract_embedded_jpeg_preview(&fake_raw).expect("Failed to extract preview");
    assert_eq!(extracted, large_jpeg);
}

#[test]
fn test_extract_embedded_jpeg_ignores_lossless_raw_streams_without_standard_sof() {
    // RAW files often have a huge lossless compressed sensor stream with SOI/EOI
    // and SOF3 (0xFFC3) or DHT (0xFFC4) without standard SOF0/SOF1/SOF2 baseline display markers.
    // The extractor must ignore this non-displayable sensor stream and pick the displayable JPEG preview.
    let mut fake_raw = Vec::new();
    fake_raw.extend_from_slice(b"RAW_HEADER");

    // Displayable preview JPEG (32x32 SOF0, size ~100 bytes)
    let mut display_preview = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x20, 0x00, 0x20, 0x01, 0x01,
    ];
    display_preview.extend_from_slice(&[0x55; 100]);
    display_preview.extend_from_slice(&[0xFF, 0xD9]);
    fake_raw.extend_from_slice(&display_preview);

    // Much larger RAW sensor stream (SOF3 / lossless 0xFFC3, size 5000 bytes)
    let mut sensor_stream = vec![
        0xFF, 0xD8, 0xFF, 0xC3, 0x00, 0x0B, 0x0E, 0x00, 0x80, 0x00, 0x80, 0x01, 0x01,
    ];
    sensor_stream.extend_from_slice(&[0xAA; 5000]);
    sensor_stream.extend_from_slice(&[0xFF, 0xD9]);
    fake_raw.extend_from_slice(&sensor_stream);

    let extracted = extract_embedded_jpeg_preview(&fake_raw)
        .expect("Must extract displayable preview ignoring sensor stream");
    assert_eq!(extracted, display_preview);
}
