//! # Tests for WebDAV FileSystem Implementation (`AvicullDavFs`)
//!
//! Verifies:
//! - Complete implementation of `dav_server::fs::DavFileSystem`.
//! - Invariant 1: Source immutability (write, delete, create_dir rejected with `FsError::Forbidden`).
//! - Metadata resolution (directories vs virtual preview files).
//! - Directory streaming (`DavDirStream`) for PROPFIND.
//! - Synthetic `/catalog.html` virtual file serving.

use std::sync::Arc;
use tempfile::tempdir;

use dav_server::davpath::DavPath;
use dav_server::fs::{DavFileSystem, FsError};
use futures_util::StreamExt;

use avicull::storage::opendal_backend::OpenDalBackend;
use avicull::storage::webdav_server::{AvicullDavFs, TaxonomicTree, VirtualAssetEntry};

#[tokio::test]
async fn test_dav_fs_read_only_invariants() {
    let tmp = tempdir().unwrap();
    let backend = Arc::new(OpenDalBackend::from_fs_path(tmp.path()).unwrap());

    let mut tree = TaxonomicTree::new();
    let entry = VirtualAssetEntry {
        order: "Passeriformes",
        family: "Turdidae",
        species_slug: "Turdus_migratorius",
        common_name: "American Robin",
        filename: "IMG_1001.JPG",
        source_uri: "sample.jpg",
        size_bytes: 4096,
    };
    tree.insert(&entry);

    let fs = AvicullDavFs::new(Arc::new(tree), backend);

    let path = DavPath::new("/Aves/Passeriformes").unwrap();

    // Invariant: Mutating methods MUST return FsError::Forbidden
    assert_eq!(
        fs.create_dir(&path).await.unwrap_err(),
        FsError::Forbidden,
        "create_dir must be rejected"
    );
    assert_eq!(
        fs.remove_dir(&path).await.unwrap_err(),
        FsError::Forbidden,
        "remove_dir must be rejected"
    );
    assert_eq!(
        fs.remove_file(&path).await.unwrap_err(),
        FsError::Forbidden,
        "remove_file must be rejected"
    );
}

#[tokio::test]
async fn test_dav_fs_metadata_and_read_dir() {
    let tmp = tempdir().unwrap();
    let sample_file = tmp.path().join("sample.jpg");
    std::fs::write(&sample_file, [0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x01]).unwrap();

    let backend = Arc::new(OpenDalBackend::from_fs_path(tmp.path()).unwrap());

    let mut tree = TaxonomicTree::new();
    let entry = VirtualAssetEntry {
        order: "Accipitriformes",
        family: "Accipitridae",
        species_slug: "Haliaeetus_leucocephalus",
        common_name: "Bald Eagle",
        filename: "IMG_2001.JPG",
        source_uri: "sample.jpg",
        size_bytes: 6,
    };
    tree.insert(&entry);

    let fs = AvicullDavFs::new(Arc::new(tree), backend);

    // Metadata on Root
    let root_path = DavPath::new("/").unwrap();
    let root_meta = fs.metadata(&root_path).await.unwrap();
    assert!(root_meta.is_dir(), "Root must be directory");

    // Read dir on Root
    let mut root_entries = fs
        .read_dir(&root_path, dav_server::fs::ReadDirMeta::None)
        .await
        .unwrap();
    let mut names = Vec::new();
    while let Some(entry) = root_entries.next().await {
        names.push(entry.unwrap().name());
    }
    assert!(names.contains(&b"Aves".to_vec()));
    assert!(
        names.contains(&b"catalog.html".to_vec()),
        "Root must contain virtual catalog.html"
    );

    // Metadata on virtual catalog.html
    let cat_path = DavPath::new("/catalog.html").unwrap();
    let cat_meta = fs.metadata(&cat_path).await.unwrap();
    assert!(cat_meta.is_file(), "catalog.html must be file");
    assert_ne!(cat_meta.len(), 0, "catalog.html must have non-zero length");
}
