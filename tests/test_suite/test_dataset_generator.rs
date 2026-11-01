use avicull::dataset::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_create_mini_dataset_from_synthetic_source() {
    let temp_root = tempdir().expect("create temp dir");
    let src_dir = temp_root.path().join("Source_Photos");
    let dst_dir = temp_root.path().join("Target_Fixtures");

    // Setup representative folder hierarchy in src_dir
    let sub1 = src_dir.join("trip_a/safari_1");
    let sub2 = src_dir.join("trip_b/walk");
    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();

    let photo1 = sub1.join("IMG_0001.jpg");
    let sidecar1 = sub1.join("IMG_0001.jpg.xmp");
    let photo2 = sub2.join("IMG_0002.jpg");

    fs::write(&photo1, b"JPEG_DATA_1").unwrap();
    fs::write(&sidecar1, b"XMP_DATA_1").unwrap();
    fs::write(&photo2, b"JPEG_DATA_2").unwrap();

    let options = MiniDatasetOptions {
        source_dir: src_dir.clone(),
        target_dir: dst_dir.clone(),
        force: true,
        curated_samples: vec![
            CuratedSample {
                image_rel_path: PathBuf::from("trip_a/safari_1/IMG_0001.jpg"),
                sidecar_rel_path: Some(PathBuf::from("trip_a/safari_1/IMG_0001.jpg.xmp")),
            },
            CuratedSample {
                image_rel_path: PathBuf::from("trip_b/walk/IMG_0002.jpg"),
                sidecar_rel_path: None,
            },
        ],
        gazetteers: vec![GazetteerFixture {
            dir_rel_path: PathBuf::from("trip_a"),
            content: "trip_name = \"Trip A\"\nlatitude = 12.0\nlongitude = 76.0\n".to_string(),
        }],
    };

    let result = create_mini_dataset(&options).expect("create mini dataset failed");
    assert_eq!(result.copied_files_count, 4); // 2 images + 1 xmp + 1 gazetteer
    assert!(dst_dir.join("trip_a/safari_1/IMG_0001.jpg").exists());
    assert!(dst_dir.join("trip_a/safari_1/IMG_0001.jpg.xmp").exists());
    assert!(dst_dir.join("trip_b/walk/IMG_0002.jpg").exists());
    assert!(dst_dir.join("trip_a/gazetteer.toml").exists());

    // Verify content matches exactly
    assert_eq!(
        fs::read(dst_dir.join("trip_a/safari_1/IMG_0001.jpg")).unwrap(),
        b"JPEG_DATA_1"
    );
    assert_eq!(
        fs::read_to_string(dst_dir.join("trip_a/gazetteer.toml")).unwrap(),
        "trip_name = \"Trip A\"\nlatitude = 12.0\nlongitude = 76.0\n"
    );
}

#[test]
fn test_create_mini_dataset_source_immutability() {
    let temp_root = tempdir().expect("create temp dir");
    let src_dir = temp_root.path().join("Source_Photos");
    let dst_dir = temp_root.path().join("Target_Fixtures");

    fs::create_dir_all(&src_dir).unwrap();
    let sample = src_dir.join("sample.jpg");
    fs::write(&sample, b"ORIGINAL_BYTES").unwrap();

    let options = MiniDatasetOptions {
        source_dir: src_dir.clone(),
        target_dir: dst_dir.clone(),
        force: true,
        curated_samples: vec![CuratedSample {
            image_rel_path: PathBuf::from("sample.jpg"),
            sidecar_rel_path: None,
        }],
        gazetteers: vec![],
    };

    create_mini_dataset(&options).expect("dataset creation failed");

    // Source file must remain completely untouched
    assert!(sample.exists());
    assert_eq!(fs::read(&sample).unwrap(), b"ORIGINAL_BYTES");
}
