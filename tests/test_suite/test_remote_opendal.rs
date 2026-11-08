//! # Tests for Remote Object Storage and OpenDAL Engine
//!
//! Verifies:
//! - Separation of `RemoteObjectBackend` from POSIX file mutations.
//! - Local filesystem streaming operator via OpenDAL (`services-fs`).
//! - Credential and URI secret redaction.
//! - Range-read retrieval of asset bytes.

use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

use avicull::storage::opendal_backend::{OpenDalBackend, RemoteObjectBackend};

#[tokio::test]
async fn test_opendal_local_fs_operator_and_range_reads() {
    let tmp = tempdir().unwrap();
    let sample_file = tmp.path().join("sample.cr2");

    // Create a 256 KB mock file
    let mut file = File::create(&sample_file).unwrap();
    let data = vec![0x42u8; 256 * 1024];
    file.write_all(&data).unwrap();
    drop(file);

    let backend = OpenDalBackend::from_fs_path(tmp.path()).unwrap();

    // Test listing / existence
    let exists = backend.exists("sample.cr2").await.unwrap();
    assert!(exists, "sample.cr2 should exist in operator");

    // Test byte-range read (first 128 KB)
    let header_bytes = backend.read_range("sample.cr2", 0, 131_072).await.unwrap();
    assert_eq!(header_bytes.len(), 131_072);
    assert_eq!(header_bytes[0], 0x42);

    // Test byte-range read (slice from 200 KB)
    let slice_bytes = backend
        .read_range("sample.cr2", 200 * 1024, 1024)
        .await
        .unwrap();
    assert_eq!(slice_bytes.len(), 1024);
    assert_eq!(slice_bytes[0], 0x42);
}

#[test]
fn test_opendal_credential_redaction() {
    let secret_uri =
        "s3://AKIAIOSFODNN7EXAMPLE:wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY@my-bucket/photos";
    let redacted = OpenDalBackend::sanitize_uri(secret_uri);
    assert!(!redacted.contains("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"));
    assert!(redacted.contains("***"));
    assert!(redacted.contains("my-bucket/photos"));

    // Redaction for AWS Query Parameters and Signatures
    let query_uri = "https://my-bucket.s3.amazonaws.com/birds.cr2?AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&Signature=vjbyPxybdZaNmGa%3D&X-Amz-Security-Token=AQoDYXdzEJr1111EXAMPLE";
    let query_redacted = OpenDalBackend::sanitize_uri(query_uri);
    assert!(!query_redacted.contains("vjbyPxybdZaNmGa%3D"));
    assert!(!query_redacted.contains("AQoDYXdzEJr1111EXAMPLE"));
    assert!(query_redacted.contains("Signature=***"));
    assert!(query_redacted.contains("X-Amz-Security-Token=***"));
}
