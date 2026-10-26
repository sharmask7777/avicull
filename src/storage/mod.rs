use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

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
}

/// Standard local APFS/HFS+ or standard POSIX filesystem volume.
pub struct LocalVolumeBackend;

impl StorageBackend for LocalVolumeBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::LocalVolume
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        let meta = std::fs::symlink_metadata(path)?;
        let file_size_bytes = meta.len();

        #[cfg(target_os = "macos")]
        {
            use std::os::darwin::fs::MetadataExt;
            const SF_DATALESS: u32 = 0x4000_0000;
            let is_dataless = (meta.st_flags() & SF_DATALESS) != 0;
            Ok(FileLocality {
                is_dataless,
                pre_existing_local: !is_dataless,
                file_size_bytes,
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(FileLocality {
                is_dataless: false,
                pre_existing_local: true,
                file_size_bytes,
            })
        }
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }

    fn evict(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}

/// macOS CloudStorage provider (OneDrive, Dropbox, Google Drive, iCloud via FileProvider).
pub struct MacOSCloudStorageBackend;

impl StorageBackend for MacOSCloudStorageBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::MacOSCloudStorage
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        LocalVolumeBackend.inspect_locality(path)
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }

    fn evict(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}

/// Network filesystem share (SMB / NFS).
pub struct NetworkShareBackend;

impl StorageBackend for NetworkShareBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::NetworkShare
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        LocalVolumeBackend.inspect_locality(path)
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }

    fn evict(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}

/// Detects the appropriate `StorageBackend` implementation based on path prefixes and mount types.
pub fn detect_storage_backend(path: &Path) -> Box<dyn StorageBackend> {
    let path_str = path.to_string_lossy();

    // 1. Check path prefix for macOS FileProvider / CloudStorage
    if path_str.contains("/Library/CloudStorage/")
        || path_str.contains("/Library/Mobile Documents/com~apple~CloudDocs")
    {
        return Box::new(MacOSCloudStorageBackend);
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
                        return Box::new(MacOSCloudStorageBackend);
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
                            return Box::new(NetworkShareBackend);
                        }
                    }
                }
            }
        }
    }

    // 4. Default to standard local volume
    Box::new(LocalVolumeBackend)
}
