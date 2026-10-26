use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use super::local::LocalVolumeBackend;
use super::{FileLocality, StorageBackend, StorageKind};

/// macOS CloudStorage provider (OneDrive, Dropbox, Google Drive, iCloud via FileProvider).
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOSCloudStorageBackend;

impl MacOSCloudStorageBackend {
    pub fn new() -> Self {
        Self
    }
}

impl StorageBackend for MacOSCloudStorageBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::MacOSCloudStorage
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        LocalVolumeBackend.inspect_locality(path)
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                sleep(Duration::from_millis(500));
            }
            match File::open(path).and_then(|mut f| {
                let mut buf = [0u8; 4096];
                let _ = f.read(&mut buf)?;
                Ok(())
            }) {
                Ok(_) => return Ok(path.to_path_buf()),
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            std::io::Error::other(format!("Failed to materialize {}", path.display()))
        }))
    }

    fn evict(&self, path: &Path) -> std::io::Result<()> {
        let path_str = path.to_string_lossy();
        let status = if path_str.contains("/Library/Mobile Documents/com~apple~CloudDocs") {
            Command::new("brctl").arg("evict").arg(path).status()
        } else {
            Command::new("fileproviderctl")
                .arg("evict")
                .arg(path)
                .status()
        };

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(std::io::Error::other(format!(
                "Eviction failed with exit status: {}",
                s
            ))),
            Err(e) => Err(e),
        }
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}
