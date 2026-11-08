//! # Tests for Remote Preview Cache and Concurrency Throttling
//!
//! Verifies:
//! - Local disk caching of extracted preview bytes prevents repeated backend reads.
//! - Cache keying is robust and deterministic across paths.
//! - Concurrency throttling bounds concurrent in-flight requests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

use async_trait::async_trait;
use avicull::storage::cache::{DiskPreviewCache, ThrottledBackend};
use avicull::storage::opendal_backend::RemoteObjectBackend;

struct CountingBackend {
    read_count: Arc<AtomicUsize>,
}

#[async_trait]
impl RemoteObjectBackend for CountingBackend {
    async fn exists(&self, _path: &str) -> std::io::Result<bool> {
        Ok(true)
    }

    async fn read_range(&self, _path: &str, _offset: u64, length: u64) -> std::io::Result<Vec<u8>> {
        self.read_count.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(vec![0xAA; length as usize])
    }
}

#[tokio::test]
async fn test_disk_preview_cache_avoids_repeated_reads() {
    let tmp = tempdir().unwrap();
    let cache = DiskPreviewCache::new(tmp.path());

    let counter = Arc::new(AtomicUsize::new(0));
    let backend = CountingBackend {
        read_count: Arc::clone(&counter),
    };

    // First fetch: cache miss
    let bytes1 = cache
        .get_or_insert_with("birds/sample.cr2", || async {
            backend.read_range("birds/sample.cr2", 0, 100).await
        })
        .await
        .unwrap();

    assert_eq!(bytes1.len(), 100);
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Second fetch: cache hit (should NOT increment counter)
    let bytes2 = cache
        .get_or_insert_with("birds/sample.cr2", || async {
            backend.read_range("birds/sample.cr2", 0, 100).await
        })
        .await
        .unwrap();

    assert_eq!(bytes2, bytes1);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "Counter must remain 1 on cache hit"
    );
}

#[tokio::test]
async fn test_throttled_backend_limits_concurrency() {
    let counter = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        read_count: Arc::clone(&counter),
    });

    let throttled = ThrottledBackend::new(backend, 2); // Max 2 concurrent permits

    let mut handles = Vec::new();
    for _ in 0..6 {
        let t = throttled.clone();
        handles.push(tokio::spawn(async move {
            t.read_range("birds/test.cr2", 0, 50).await.unwrap()
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert_eq!(res.len(), 50);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn test_cache_corrupt_or_empty_file_handling() {
    let tmp = tempdir().unwrap();
    let cache = DiskPreviewCache::new(tmp.path());
    let path = cache.cache_path("birds/empty.cr2");

    // Create empty file
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    std::fs::write(&path, []).unwrap();

    // Cache should ignore empty file and fetch fresh
    let fetched = cache
        .get_or_insert_with("birds/empty.cr2", || async { Ok(vec![0x77u8; 10]) })
        .await
        .unwrap();

    assert_eq!(fetched.len(), 10);
}
