//! # Abstract Storage Layer & Storage Provider Detection
//!
//! Photography archives are often stored on a mixture of local APFS volumes, external USB/Thunderbolt drives,
//! NAS shares (SMB/NFS), and cloud-synchronized file provider folders (iCloud Drive, Proton Drive, OneDrive, Google Drive).
//!
//! This module provides a polymorphic storage abstraction ([`StorageBackend`]) that encapsulates:
//! - **Locality Inspection**: Distinguishing between physically hydrated local blocks and macOS FileProvider dataless stubs (`SF_DATALESS`).
//! - **On-Demand Materialization**: Safely streaming/hydrating remote cloud files into temporary cache when required for EXIF or neural classification.
//! - **Idempotent Eviction**: Purging downloaded blocks after classification without evicting files that were already resident before avicull executed.
//! - **Filesystem Safety**: Detecting cross-device boundaries and rejecting unsafe hardlink modes on remote shares.

pub mod cloud;
pub mod local;
pub mod network;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub use cloud::MacOSCloudStorageBackend;
pub use local::LocalVolumeBackend;
pub use network::NetworkShareBackend;

/// Identifies the underlying filesystem or storage provider category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// macOS CloudStorage / FileProvider (iCloud Drive, Proton Drive, OneDrive, Dropbox).
    MacOSCloudStorage,
    /// Network-attached storage mounted via SMB, NFS, or AFP.
    NetworkShare,
    /// Direct-attached local volume (APFS, HFS+, ExFAT).
    LocalVolume,
}

/// Represents the physical locality and hydration state of an asset on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLocality {
    /// Whether the file is an unhydrated dataless fault stub on macOS FileProvider.
    pub is_dataless: bool,
    /// Whether the file was already fully resident on local disk before avicull inspected it.
    pub pre_existing_local: bool,
    /// Total logical size of the asset payload in bytes.
    pub file_size_bytes: u64,
}

/// Abstract storage backend managing file hydration, eviction, and asset moves.
pub trait StorageBackend: Send + Sync {
    /// Returns the storage category for this backend.
    fn kind(&self) -> StorageKind;

    /// Inspects whether the target file is dataless (cloud stub) or resident on local disk.
    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality>;

    /// Hydrates/downloads the asset so it is physically present on disk.
    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf>;

    /// Evicts the asset's local disk blocks back to the cloud/remote tier.
    fn evict(&self, path: &Path) -> std::io::Result<()>;

    /// Moves/renames an asset across directories within the backend.
    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()>;

    /// Selectively evicts the asset only if it was downloaded by avicull.
    ///
    /// # Critical Invariant
    /// If the file was already resident locally before avicull ran (`locality.pre_existing_local == true`),
    /// this method MUST be a no-op to avoid unintentionally purging user data blocks.
    fn evict_if_downloaded(&self, path: &Path, locality: &FileLocality) -> std::io::Result<()> {
        if locality.pre_existing_local {
            return Ok(());
        }
        self.evict(path)
    }

    /// Safely evicts hydrated blocks *before* renaming an asset.
    ///
    /// For cloud storage providers, performing eviction before renaming
    /// ensures clean file state transitions without sync lockouts.
    fn evict_and_rename(
        &self,
        src: &Path,
        dst: &Path,
        locality: &FileLocality,
    ) -> std::io::Result<()> {
        self.evict_if_downloaded(src, locality)?;
        self.rename_asset(src, dst)
    }
}

/// Automatically detects the appropriate `StorageBackend` implementation based on path prefixes and mount types.
///
/// Detection cascade:
/// 1. Path contains `/Library/CloudStorage/` or iCloud Drive paths $\to$ [`MacOSCloudStorageBackend`].
/// 2. `statfs` filesystem type indicates SMB/NFS $\to$ [`NetworkShareBackend`].
/// 3. Otherwise $\to$ [`LocalVolumeBackend`].
pub fn detect_storage_backend(path: &Path) -> Box<dyn StorageBackend> {
    let path_str = path.to_string_lossy();

    // 1. Check path prefix for macOS FileProvider / CloudStorage
    if path_str.contains("/Library/CloudStorage/")
        || path_str.contains("/Library/Mobile Documents/com~apple~CloudDocs")
    {
        return Box::new(MacOSCloudStorageBackend::new());
    }

    // 2. Check statfs for network filesystem types
    if is_network_mount(path) {
        return Box::new(NetworkShareBackend::new());
    }

    // 3. Default to local volume backend
    Box::new(LocalVolumeBackend::new())
}

/// Checks whether a given path is mounted on a network share (SMB, NFS, AFP).
fn is_network_mount(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut resolved = path.to_path_buf();
        while !resolved.exists() && resolved.parent().is_some() {
            resolved = resolved.parent().unwrap().to_path_buf();
        }

        let c_path = match CString::new(resolved.as_os_str().as_bytes()) {
            Ok(c) => c,
            Err(_) => return false,
        };

        unsafe {
            let mut stat: libc::statfs = std::mem::zeroed();
            if libc::statfs(c_path.as_ptr(), &mut stat) == 0 {
                let fstype_bytes: Vec<u8> = stat
                    .f_fstypename
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8)
                    .collect();
                let fstype = String::from_utf8_lossy(&fstype_bytes).to_lowercase();
                if fstype.contains("smbfs")
                    || fstype.contains("nfs")
                    || fstype.contains("afpfs")
                    || fstype.contains("webdav")
                {
                    return true;
                }
            }
        }
    }
    let _ = path;
    false
}
