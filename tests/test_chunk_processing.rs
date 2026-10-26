use avicull::organizer::chunk::{parse_size_budget, partition_into_chunks, ChunkBudget};
use avicull::storage::LocalVolumeBackend;
use avicull::types::AssetBundle;
use std::path::PathBuf;

#[test]
fn test_chunk_budget_struct() {
    let budget = ChunkBudget::new(5 * 1024 * 1024);
    assert_eq!(budget.max_bytes, 5 * 1024 * 1024);
}

fn create_mock_bundle(id: &str, file_path: PathBuf) -> AssetBundle {
    AssetBundle {
        bundle_id: id.to_string(),
        source_inode: 1234,
        trip_folder_name: "TestTrip".to_string(),
        original_stem: id.to_string(),
        primary_image: file_path,
        companion_images: Vec::new(),
        sidecar_files: Vec::new(),
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    }
}

#[test]
fn test_parse_size_budget() {
    assert_eq!(parse_size_budget("5GB").unwrap(), 5 * 1024 * 1024 * 1024);
    assert_eq!(parse_size_budget("10gb").unwrap(), 10 * 1024 * 1024 * 1024);
    assert_eq!(parse_size_budget("500MB").unwrap(), 500 * 1024 * 1024);
    assert_eq!(parse_size_budget("1024KB").unwrap(), 1024 * 1024);
    assert_eq!(parse_size_budget("1048576").unwrap(), 1048576);
    assert_eq!(parse_size_budget("2GiB").unwrap(), 2 * 1024 * 1024 * 1024);
    assert_eq!(parse_size_budget("100MiB").unwrap(), 100 * 1024 * 1024);
    assert_eq!(parse_size_budget("50KiB").unwrap(), 50 * 1024);
    assert!(parse_size_budget("").is_err());
    assert!(parse_size_budget("invalid").is_err());
    assert!(parse_size_budget("-5GB").is_err());
}

#[test]
fn test_chunk_partitioning_respects_cache_budget() {
    let temp = tempfile::tempdir().unwrap();
    let backend = LocalVolumeBackend::new();

    // Create 5 mock bundle files, each 50 bytes (for testing)
    let mut bundles = Vec::new();
    for i in 1..=5 {
        let file_path = temp.path().join(format!("img_{}.jpg", i));
        std::fs::write(&file_path, vec![0u8; 50]).unwrap();
        bundles.push(create_mock_bundle(&format!("bundle_{}", i), file_path));
    }

    // Partition with 120 bytes limit:
    // Chunk 1: bundle 1 (50) + bundle 2 (50) = 100 <= 120
    // Chunk 2: bundle 3 (50) + bundle 4 (50) = 100 <= 120
    // Chunk 3: bundle 5 (50) = 50 <= 120
    let chunks = partition_into_chunks(bundles, 120, &backend);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 2);
    assert_eq!(chunks[1].len(), 2);
    assert_eq!(chunks[2].len(), 1);
}

#[test]
fn test_chunk_partitioning_single_file_exceeds_budget() {
    let temp = tempfile::tempdir().unwrap();
    let backend = LocalVolumeBackend::new();

    let p1 = temp.path().join("img_1.jpg");
    std::fs::write(&p1, vec![0u8; 200]).unwrap(); // 200 bytes

    let p2 = temp.path().join("img_2.jpg");
    std::fs::write(&p2, vec![0u8; 30]).unwrap(); // 30 bytes

    let bundles = vec![create_mock_bundle("b1", p1), create_mock_bundle("b2", p2)];

    // Partition with 100 bytes limit:
    // b1 is 200 bytes > 100, so it gets its own chunk of 1 item
    // b2 is 30 bytes <= 100, goes in next chunk
    let chunks = partition_into_chunks(bundles, 100, &backend);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].len(), 1);
    assert_eq!(chunks[0][0].bundle_id, "b1");
    assert_eq!(chunks[1].len(), 1);
    assert_eq!(chunks[1][0].bundle_id, "b2");
}

#[test]
fn test_mode_hierarchy_unsupported_on_remote_backend() {
    use avicull::storage::cloud::MacOSCloudStorageBackend;
    use avicull::types::OrganizationMode;

    let backend = MacOSCloudStorageBackend::new();
    let res = avicull::organizer::validate_organization_mode_for_backend(
        &backend,
        OrganizationMode::HierarchicalHardlinkHierarchy,
    );
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("cloud-synchronized") || err_msg.contains("not supported"));
}

#[test]
fn test_cli_organize_rejects_hierarchy_mode_on_cloud_storage() {
    use std::fs::File;
    use std::io::Write;
    use std::process::Command;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    // Simulate cloud storage path by including /Library/CloudStorage/
    let fake_cloud_dir = temp
        .path()
        .join("Library")
        .join("CloudStorage")
        .join("OneDrive")
        .join("Photos");
    std::fs::create_dir_all(&fake_cloud_dir).unwrap();

    let img_path = fake_cloud_dir.join("photo.jpg");
    File::create(&img_path).unwrap().write_all(b"test").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            fake_cloud_dir.to_str().unwrap(),
            "--mode",
            "hierarchy",
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");

    assert!(
        !output.status.success(),
        "organize should fail when hierarchy mode is requested on cloud storage"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not supported") || stderr.contains("cloud-synchronized"));
}

#[test]
fn test_cli_organize_in_place_chunked_processing_evicts_downloaded() {
    use std::fs::File;
    use std::io::Write;
    use std::process::Command;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let input_dir = temp.path().join("Photos");
    std::fs::create_dir_all(&input_dir).unwrap();

    let img1 = input_dir.join("photo1.jpg");
    File::create(&img1)
        .unwrap()
        .write_all(b"test jpeg data 1")
        .unwrap();
    let img2 = input_dir.join("photo2.jpg");
    File::create(&img2)
        .unwrap()
        .write_all(b"test jpeg data 2")
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--max-cache-size",
            "100B",
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");

    assert!(
        output.status.success(),
        "organize should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(input_dir.join("bird_catalog.html").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Processing chunk"),
        "stdout should show chunked processing loop: {}",
        stdout
    );
}
