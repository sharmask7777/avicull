use std::path::{Path, PathBuf};

use super::{FileLocality, StorageBackend, StorageKind};

/// Standard local APFS/HFS+ or standard POSIX filesystem volume.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalVolumeBackend;

impl LocalVolumeBackend {
    pub fn new() -> Self {
        Self
    }
}

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
