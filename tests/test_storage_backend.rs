use avicull::storage::cloud::MacOSCloudStorageBackend;
use avicull::storage::network::NetworkShareBackend;
use avicull::storage::{detect_storage_backend, FileLocality, StorageBackend, StorageKind};
use std::path::Path;

#[test]
fn test_local_volume_detection() {
    let temp = tempfile::tempdir().unwrap();
    let backend = detect_storage_backend(temp.path());
    assert_eq!(backend.kind(), StorageKind::LocalVolume);

    let file = temp.path().join("test.jpg");
    std::fs::write(&file, b"test image payload").unwrap();
    let locality = backend.inspect_locality(&file).unwrap();
    assert!(!locality.is_dataless);
    assert!(locality.pre_existing_local);
    assert_eq!(locality.file_size_bytes, 18);
}

#[test]
fn test_local_volume_materialize_and_evict() {
    let temp = tempfile::tempdir().unwrap();
    let backend = detect_storage_backend(temp.path());

    let file = temp.path().join("test.jpg");
    std::fs::write(&file, b"hello").unwrap();

    let materialized = backend.materialize(&file).unwrap();
    assert_eq!(materialized, file);

    // Eviction on LocalVolume is a safe no-op
    assert!(backend.evict(&file).is_ok());
    assert!(file.exists());
}

#[test]
fn test_local_volume_rename_asset() {
    let temp = tempfile::tempdir().unwrap();
    let backend = detect_storage_backend(temp.path());

    let src = temp.path().join("source.jpg");
    let dst = temp.path().join("dest.jpg");
    std::fs::write(&src, b"data").unwrap();

    backend.rename_asset(&src, &dst).unwrap();
    assert!(!src.exists());
    assert!(dst.exists());
}

#[test]
fn test_cloud_storage_path_detection() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/test".to_string());
    let cloud_path = format!("{}/Library/CloudStorage/OneDrive-Personal/Photos", home);
    let backend = detect_storage_backend(Path::new(&cloud_path));
    assert_eq!(backend.kind(), StorageKind::MacOSCloudStorage);
}

#[test]
fn test_selective_eviction_skips_preexisting_local_files() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("local.jpg");
    std::fs::write(&file, b"local data").unwrap();

    let backend = MacOSCloudStorageBackend::new();
    let locality = FileLocality {
        is_dataless: false,
        pre_existing_local: true,
        file_size_bytes: 10,
    };
    // Calling evict_if_downloaded on a file marked pre_existing_local must be a no-op
    assert!(backend.evict_if_downloaded(&file, &locality).is_ok());
    assert!(file.exists());
}

#[test]
fn test_network_share_staging_materialize_and_evict() {
    let temp = tempfile::tempdir().unwrap();
    let remote_file = temp.path().join("remote_image.cr2");
    std::fs::write(&remote_file, b"raw payload on remote share").unwrap();

    let backend = NetworkShareBackend::new();
    let locality = backend.inspect_locality(&remote_file).unwrap();

    // Materialize streams the file into staging area on local SSD
    let staged_path = backend.materialize(&remote_file).unwrap();
    assert!(staged_path.exists());
    assert_ne!(staged_path, remote_file);
    assert_eq!(
        std::fs::read(&staged_path).unwrap(),
        b"raw payload on remote share"
    );

    // Eviction on network share cleans up the staged file
    let download_locality = FileLocality {
        is_dataless: true,
        pre_existing_local: false,
        file_size_bytes: locality.file_size_bytes,
    };
    assert!(backend
        .evict_if_downloaded(&remote_file, &download_locality)
        .is_ok());
    assert!(!staged_path.exists());
    assert!(remote_file.exists());
}

#[test]
fn test_evict_and_rename_ordering() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("src.jpg");
    let dst = temp.path().join("dst.jpg");
    std::fs::write(&src, b"data to rename").unwrap();

    let backend = MacOSCloudStorageBackend::new();
    let locality = FileLocality {
        is_dataless: false,
        pre_existing_local: true,
        file_size_bytes: 14,
    };

    assert!(backend.evict_and_rename(&src, &dst, &locality).is_ok());
    assert!(!src.exists());
    assert!(dst.exists());
}
