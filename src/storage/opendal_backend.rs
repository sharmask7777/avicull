//! # Apache OpenDAL Remote Storage Backend
//!
//! Provides an asynchronous, range-query-capable remote storage abstraction ([`RemoteObjectBackend`])
//! powered by Apache OpenDAL.
//!
//! ## Architectural Invariants
//! - **Strict Source Immutability**: Remote object storage is read-only. No mutations, deletions, or renames.
//! - **Decoupled from POSIX Hardlinks**: Does not implement local filesystem mutations or APFS hard links.
//! - **Zero Credential Leaks**: Strips authentication secrets and access keys from URIs and error channels.

use async_trait::async_trait;
use opendal::{services, Operator};
use std::path::Path;

/// Trait defining read-only remote object operations and range reading.
#[async_trait]
pub trait RemoteObjectBackend: Send + Sync {
    /// Checks whether an object exists at the given relative key/path.
    async fn exists(&self, path: &str) -> std::io::Result<bool>;

    /// Reads a specific byte range `[offset, offset + length)` from the target object.
    async fn read_range(&self, path: &str, offset: u64, length: u64) -> std::io::Result<Vec<u8>>;
}

/// Storage backend powered by Apache OpenDAL.
pub struct OpenDalBackend {
    operator: Operator,
}

impl OpenDalBackend {
    /// Creates a backend from an initialized OpenDAL operator.
    #[must_use]
    pub fn new(operator: Operator) -> Self {
        Self { operator }
    }

    /// Initializes an OpenDAL backend pointing to a local filesystem directory.
    ///
    /// This enables zero-cloud local mock testing and testing on local shares.
    ///
    /// # Errors
    /// Returns an IO error if operator initialization fails.
    pub fn from_fs_path(path: &Path) -> std::io::Result<Self> {
        let mut builder = services::Fs::default();
        builder = builder.root(&path.to_string_lossy());

        let operator = Operator::new(builder)
            .map_err(|e| std::io::Error::other(format!("Failed to build Fs operator: {e}")))?;

        Ok(Self::new(operator))
    }

    /// Initializes an OpenDAL backend pointing to an S3 or S3-compatible (MinIO/R2) bucket.
    ///
    /// # Arguments
    /// * `bucket` - S3 bucket name.
    /// * `endpoint` - Optional custom endpoint URL (e.g. for MinIO or Cloudflare R2).
    /// * `region` - Optional AWS region (defaults to us-east-1).
    ///
    /// # Errors
    /// Returns an IO error if operator initialization fails.
    pub fn from_s3(
        bucket: &str,
        endpoint: Option<&str>,
        region: Option<&str>,
    ) -> std::io::Result<Self> {
        let mut builder = services::S3::default();
        builder = builder.bucket(bucket);
        if let Some(ep) = endpoint {
            builder = builder.endpoint(ep);
        }
        if let Some(reg) = region {
            builder = builder.region(reg);
        }

        let operator = Operator::new(builder).map_err(|e| {
            let safe_msg = Self::sanitize_uri(&e.to_string());
            std::io::Error::other(format!("Failed to build S3 operator: {safe_msg}"))
        })?;

        Ok(Self::new(operator))
    }

    /// Redacts credentials, secret access keys, query signatures, and basic auth from URI strings and error messages.
    #[must_use]
    pub fn sanitize_uri(uri: &str) -> String {
        let mut sanitized = uri.to_string();

        // 1. Redact user:password@host -> user:***@host
        if let Some(at_idx) = sanitized.find('@') {
            if let Some(scheme_idx) = sanitized.find("://") {
                let pre_scheme = sanitized[..scheme_idx + 3].to_string();
                let user_info = &sanitized[scheme_idx + 3..at_idx];
                let post_at = sanitized[at_idx..].to_string();
                if let Some(colon_idx) = user_info.find(':') {
                    let user = &user_info[..colon_idx];
                    sanitized = format!("{pre_scheme}{user}:***{post_at}");
                } else {
                    sanitized = format!("{pre_scheme}***{post_at}");
                }
            }
        }

        // 2. Redact sensitive query parameters (Signature, AWSAccessKeyId, X-Amz-Security-Token, etc.)
        let sensitive_params = [
            "Signature=",
            "X-Amz-Signature=",
            "X-Amz-Security-Token=",
            "AWSAccessKeyId=",
            "secret_key=",
            "access_key=",
        ];

        for param in sensitive_params {
            let mut search_from = 0;
            while let Some(rel_start) = sanitized[search_from..].find(param) {
                let start = search_from + rel_start;
                let val_start = start + param.len();
                let val_end = sanitized[val_start..]
                    .find('&')
                    .map_or(sanitized.len(), |idx| val_start + idx);
                let before = &sanitized[..val_start];
                let after = &sanitized[val_end..];
                sanitized = format!("{before}***{after}");
                search_from = val_start + 3;
            }
        }

        sanitized
    }
}

#[async_trait]
impl RemoteObjectBackend for OpenDalBackend {
    async fn exists(&self, path: &str) -> std::io::Result<bool> {
        self.operator
            .exists(path)
            .await
            .map_err(|e| std::io::Error::other(Self::sanitize_uri(&e.to_string())))
    }

    async fn read_range(&self, path: &str, offset: u64, length: u64) -> std::io::Result<Vec<u8>> {
        let meta = self
            .operator
            .stat(path)
            .await
            .map_err(|e| std::io::Error::other(Self::sanitize_uri(&e.to_string())))?;

        let file_len = meta.content_length();
        if offset >= file_len {
            return Ok(Vec::new());
        }

        let actual_end = (offset.saturating_add(length)).min(file_len);
        let bytes = self
            .operator
            .read_with(path)
            .range(offset..actual_end)
            .await
            .map_err(|e| std::io::Error::other(Self::sanitize_uri(&e.to_string())))?;
        Ok(bytes.to_vec())
    }
}
