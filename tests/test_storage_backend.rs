use avicull::storage::{detect_storage_backend, StorageKind};
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
