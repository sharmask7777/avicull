//! # Chunked Processing & Cache Memory Budgeting
//!
//! When processing tens of thousands of RAW photos or streaming assets from network/cloud storage,
//! loading everything concurrently can exhaust memory or saturate local scratch disks.
//!
//! This module partitions photo archives into discrete sequential processing chunks bounded by a byte budget:
//! - [`parse_size_budget`]: Parses human-readable size limits (e.g. `"5GB"`, `"500MB"`).
//! - [`partition_into_chunks`]: Groups bundles into batches staying under `max_bytes`, keeping bursts
//!   together where possible while isolating oversized items so execution never deadlocks.

use crate::storage::StorageBackend;
use crate::types::AssetBundle;
use anyhow::{anyhow, Result};

/// Budget specifying the maximum cumulative size in bytes for a processing chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkBudget {
    pub max_bytes: u64,
}

impl ChunkBudget {
    pub fn new(max_bytes: u64) -> Self {
        Self { max_bytes }
    }
}

/// Parses human-readable size strings (e.g. "5GB", "500MB", "1024KB", "1048576") into byte counts.
pub fn parse_size_budget(size_str: &str) -> Result<u64> {
    let trimmed = size_str.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Empty size budget string"));
    }

    let upper = trimmed.to_uppercase();
    let (num_str, multiplier) = if upper.ends_with("GIB") || upper.ends_with("GB") {
        let suffix_len = if upper.ends_with("GIB") { 3 } else { 2 };
        (&upper[..upper.len() - suffix_len], 1024 * 1024 * 1024u64)
    } else if upper.ends_with("MIB") || upper.ends_with("MB") {
        let suffix_len = if upper.ends_with("MIB") { 3 } else { 2 };
        (&upper[..upper.len() - suffix_len], 1024 * 1024u64)
    } else if upper.ends_with("KIB") || upper.ends_with("KB") {
        let suffix_len = if upper.ends_with("KIB") { 3 } else { 2 };
        (&upper[..upper.len() - suffix_len], 1024u64)
    } else if upper.ends_with('B')
        && !upper.ends_with("KB")
        && !upper.ends_with("MB")
        && !upper.ends_with("GB")
    {
        (&upper[..upper.len() - 1], 1u64)
    } else {
        (upper.as_str(), 1u64)
    };

    let val: u64 = num_str
        .trim()
        .parse()
        .map_err(|e| anyhow!("Invalid size format '{}': {}", size_str, e))?;

    val.checked_mul(multiplier)
        .ok_or_else(|| anyhow!("Size overflow in parsing: {}", size_str))
}

/// Partitions asset bundles into batches whose cumulative file sizes stay within `max_bytes`.
///
/// Bundles from the same trip within a short timestamp window (bursts) are kept together
/// where possible within the cache budget.
/// Any single bundle whose total file size exceeds `max_bytes` is placed into its own
/// isolated single-item chunk so progress is never blocked.
pub fn partition_into_chunks(
    bundles: Vec<AssetBundle>,
    max_bytes: u64,
    backend: &dyn StorageBackend,
) -> Vec<Vec<AssetBundle>> {
    if bundles.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_bytes = 0u64;

    for bundle in bundles {
        // Compute bundle total size from inspect_locality
        let mut bundle_size = 0u64;
        if let Ok(loc) = backend.inspect_locality(&bundle.primary_image) {
            bundle_size += loc.file_size_bytes;
        }
        for comp in &bundle.companion_images {
            if let Ok(loc) = backend.inspect_locality(comp) {
                bundle_size += loc.file_size_bytes;
            }
        }
        for sidecar in &bundle.sidecar_files {
            if let Ok(loc) = backend.inspect_locality(sidecar) {
                bundle_size += loc.file_size_bytes;
            }
        }

        // Single bundle larger than max_bytes gets its own chunk
        if bundle_size > max_bytes {
            if !current_chunk.is_empty() {
                chunks.push(std::mem::take(&mut current_chunk));
                current_bytes = 0;
            }
            chunks.push(vec![bundle]);
            continue;
        }

        if current_bytes + bundle_size > max_bytes && !current_chunk.is_empty() {
            chunks.push(std::mem::take(&mut current_chunk));
            current_bytes = 0;
        }

        current_bytes += bundle_size;
        current_chunk.push(bundle);
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}
