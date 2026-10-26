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
    MacOSCloudStorage,
    NetworkShare,
    LocalVolume,
}

/// Represents the physical locality and hydration state of an asset on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLocality {
    pub is_dataless: bool,
    pub pre_existing_local: bool,
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
    /// Critical invariant: if the file was already resident locally before
    /// avicull ran (`locality.pre_existing_local == true`), this method MUST
    /// be a no-op to avoid evicting user files.
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

/// Detects the appropriate `StorageBackend` implementation based on path prefixes and mount types.
pub fn detect_storage_backend(path: &Path) -> Box<dyn StorageBackend> {
    let path_str = path.to_string_lossy();

    // 1. Check path prefix for macOS FileProvider / CloudStorage
    if path_str.contains("/Library/CloudStorage/")
        || path_str.contains("/Library/Mobile Documents/com~apple~CloudDocs")
    {
        return Box::new(MacOSCloudStorageBackend::new());
    }

    // 2. Check extended attributes if path exists
    if path.exists() {
        if let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) {
            #[cfg(target_os = "macos")]
            unsafe {
                let mut buf = [0u8; 1024];
                let list_len = libc::listxattr(
                    c_path.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_char,
                    buf.len(),
                    libc::XATTR_NOFOLLOW,
                );
                if list_len > 0 {
                    let slice = &buf[..list_len as usize];
                    let xattrs_str = String::from_utf8_lossy(slice);
                    if xattrs_str.contains("com.apple.fileprovider")
                        || xattrs_str.contains("com.apple.CloudDocs")
                    {
                        return Box::new(MacOSCloudStorageBackend::new());
                    }
                }
            }
        }
    }

    // 3. Check filesystem mount type via statfs (e.g. smbfs, nfs)
    #[cfg(unix)]
    {
        let check_path = if path.exists() {
            Some(path.to_path_buf())
        } else {
            path.parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
        };

        if let Some(target) = check_path {
            if let Ok(c_path) = CString::new(target.as_os_str().as_bytes()) {
                unsafe {
                    let mut stat: libc::statfs = std::mem::zeroed();
                    if libc::statfs(c_path.as_ptr(), &mut stat) == 0 {
                        let fs_type =
                            std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()).to_string_lossy();
                        if fs_type == "smbfs" || fs_type == "nfs" {
                            return Box::new(NetworkShareBackend::new());
                        }
                    }
                }
            }
        }
    }

    // 4. Default to standard local volume
    Box::new(LocalVolumeBackend::new())
}
