use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

use super::local::LocalVolumeBackend;
use super::{FileLocality, StorageBackend, StorageKind};

/// Network filesystem share (SMB / NFS).
///
/// Remote files are streamed on demand into a local staging directory
/// under `$TMPDIR/avicull_staging/` to allow fast random access without
/// saturating network round-trips.
#[derive(Debug)]
pub struct NetworkShareBackend {
    staging_dir: PathBuf,
    staged_map: Mutex<HashMap<PathBuf, PathBuf>>,
}

impl Default for NetworkShareBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkShareBackend {
    pub fn new() -> Self {
        let tmp = std::env::temp_dir();
        let staging_dir = tmp.join("avicull_staging");
        Self::with_staging_dir(staging_dir)
    }

    pub fn with_staging_dir(staging_dir: PathBuf) -> Self {
        Self {
            staging_dir,
            staged_map: Mutex::new(HashMap::new()),
        }
    }

    fn staging_path_for(&self, remote_path: &Path) -> PathBuf {
        let hash = blake3::hash(remote_path.to_string_lossy().as_bytes());
        let hash_hex = &hash.to_hex()[..16];
        let file_name = remote_path
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "asset".into());

        self.staging_dir.join(format!("{}_{}", hash_hex, file_name))
    }
}

impl StorageBackend for NetworkShareBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::NetworkShare
    }

    fn inspect_locality(&self, path: &Path) -> std::io::Result<FileLocality> {
        LocalVolumeBackend.inspect_locality(path)
    }

    fn materialize(&self, path: &Path) -> std::io::Result<PathBuf> {
        let mut map = self
            .staged_map
            .lock()
            .map_err(|_| std::io::Error::other("Staging lock poisoned"))?;

        if let Some(existing) = map.get(path) {
            if existing.exists() {
                return Ok(existing.clone());
            }
        }

        std::fs::create_dir_all(&self.staging_dir)?;
        let staged_path = self.staging_path_for(path);

        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                sleep(Duration::from_millis(500));
            }

            match (|| -> std::io::Result<()> {
                let mut src = File::open(path)?;
                let mut dst = File::create(&staged_path)?;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = src.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut dst, &buf[..n])?;
                }
                dst.sync_all()?;
                Ok(())
            })() {
                Ok(_) => {
                    map.insert(path.to_path_buf(), staged_path.clone());
                    return Ok(staged_path);
                }
                Err(err) => {
                    if staged_path.exists() {
                        let _ = std::fs::remove_file(&staged_path);
                    }
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            std::io::Error::other(format!("Failed to stream network asset {}", path.display()))
        }))
    }

    fn evict(&self, path: &Path) -> std::io::Result<()> {
        let mut map = self
            .staged_map
            .lock()
            .map_err(|_| std::io::Error::other("Staging lock poisoned"))?;

        if let Some(staged) = map.remove(path) {
            if staged.exists() {
                std::fs::remove_file(&staged)?;
            }
        } else {
            let direct = self.staging_path_for(path);
            if direct.exists() {
                std::fs::remove_file(&direct)?;
            }
        }

        Ok(())
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::rename(src, dst)
    }
}
