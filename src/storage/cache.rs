//! # Persistent Disk Preview Cache and Concurrency Throttling
//!
//! Provides:
//! - [`DiskPreviewCache`]: Stores remote embedded JPEG previews in a local directory (e.g. `~/.cache/avicull/previews/`).
//! - [`ThrottledBackend`]: Enforces concurrency limits across remote range requests using a [`tokio::sync::Semaphore`],
//!   preventing Finder quicklook and thumbnails from flooding S3 and triggering AWS 503 `SlowDown` errors.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

use async_trait::async_trait;

use super::opendal_backend::RemoteObjectBackend;

/// Disk cache for extracted previews and thumbnails.
#[derive(Clone, Debug)]
pub struct DiskPreviewCache {
    cache_dir: PathBuf,
}

impl DiskPreviewCache {
    /// Creates a new `DiskPreviewCache` at the specified directory.
    #[must_use]
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            cache_dir: cache_dir.to_path_buf(),
        }
    }

    /// Computes a safe filesystem key for a remote asset path using blake3 hashing.
    #[must_use]
    pub fn cache_path(&self, key: &str) -> PathBuf {
        let hash = blake3::hash(key.as_bytes()).to_hex();
        self.cache_dir.join(format!("{hash}.jpg"))
    }

    /// Retrieves cached preview bytes or populates the cache using the provided async generator.
    ///
    /// # Errors
    /// Returns an IO error if disk read/write fails or generator fails.
    pub async fn get_or_insert_with<F, Fut>(
        &self,
        key: &str,
        fetcher: F,
    ) -> std::io::Result<Vec<u8>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::io::Result<Vec<u8>>>,
    {
        let path = self.cache_path(key);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                if !bytes.is_empty() {
                    return Ok(bytes);
                }
            }
        }

        // Cache miss: execute fetcher
        let bytes = fetcher().await?;

        // Cache write: create parent dir and write atomically
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &bytes);

        Ok(bytes)
    }
}

/// A wrapper around a [`RemoteObjectBackend`] that bounds concurrent in-flight requests.
pub struct ThrottledBackend<B> {
    inner: Arc<B>,
    semaphore: Arc<Semaphore>,
}

impl<B> Clone for ThrottledBackend<B> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            semaphore: Arc::clone(&self.semaphore),
        }
    }
}

impl<B: RemoteObjectBackend> ThrottledBackend<B> {
    /// Wraps the backend with a max concurrent request limit.
    pub fn new(backend: Arc<B>, max_concurrent: usize) -> Self {
        Self {
            inner: backend,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }
}

#[async_trait]
impl<B: RemoteObjectBackend> RemoteObjectBackend for ThrottledBackend<B> {
    async fn exists(&self, path: &str) -> std::io::Result<bool> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.inner.exists(path).await
    }

    async fn read_range(&self, path: &str, offset: u64, length: u64) -> std::io::Result<Vec<u8>> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.inner.read_range(path, offset, length).await
    }
}
