//! # Loopback WebDAV Mount Service & Lifecycle Management
//!
//! Provides:
//! - [`MountService`]: Spawns loopback WebDAV HTTP service with ephemeral credentials.
//! - Darwin helpers: Runs `/sbin/mount_webdav` and `/usr/sbin/diskutil unmount`.
//! - [`generate_auth_token`]: Generates secure 128-bit hex tokens for localhost loopback auth.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::TcpListener;

use super::opendal_backend::RemoteObjectBackend;
use super::webdav_server::{AvicullDavFs, TaxonomicTree};

/// Configuration options for the mount service.
#[derive(Debug, Clone)]
pub struct MountServiceConfig {
    /// Port to bind (0 for ephemeral dynamic allocation).
    pub port: u16,
    /// Destination mount point in the filesystem (e.g. `/Volumes/avicull_birds`).
    pub mount_point: PathBuf,
    /// Whether to run in serve-only mode without mounting to the OS filesystem.
    pub serve_only: bool,
    /// Ephemeral basic auth token for the session.
    pub auth_token: String,
}

/// Generates a 32-character hexadecimal random authentication token.
#[must_use]
pub fn generate_auth_token() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let rand_val = std::process::id();
    let hash = blake3::hash(format!("{now}-{rand_val}-avicull-auth").as_bytes());
    hash.to_hex()[..32].to_string()
}

/// Lifecycle manager for WebDAV loopback and OS mount points.
pub struct MountService<B: RemoteObjectBackend> {
    config: MountServiceConfig,
    tree: Arc<TaxonomicTree>,
    backend: Arc<B>,
}

impl<B: RemoteObjectBackend + 'static> MountService<B> {
    /// Constructs a new `MountService`.
    pub fn new(config: MountServiceConfig, tree: Arc<TaxonomicTree>, backend: Arc<B>) -> Self {
        Self {
            config,
            tree,
            backend,
        }
    }

    /// Binds the loopback TCP listener to `127.0.0.1:<port>`.
    ///
    /// # Errors
    /// Returns an IO error if socket binding fails.
    pub async fn bind_listener(&self) -> std::io::Result<SocketAddr> {
        let addr = format!("127.0.0.1:{}", self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        listener.local_addr()
    }

    /// Creates the dav-server handler configured with `AvicullDavFs`.
    #[must_use]
    pub fn create_dav_handler(&self) -> dav_server::DavHandler {
        let dav_fs = Box::new(AvicullDavFs::new(
            Arc::clone(&self.tree),
            Arc::clone(&self.backend),
        ));
        dav_server::DavHandler::builder()
            .filesystem(dav_fs)
            .build_handler()
    }

    /// Invokes `/sbin/mount_webdav` on macOS.
    ///
    /// # Arguments
    /// * `port` - The local port the WebDAV server is listening on.
    /// * `mount_point` - Target directory path for mount.
    /// * `auth_token` - Ephemeral session auth token.
    ///
    /// # Errors
    /// Returns an error if directory creation or mounting fails.
    #[cfg(target_os = "macos")]
    pub fn mount_darwin(
        port: u16,
        mount_point: &std::path::Path,
        auth_token: &str,
    ) -> std::io::Result<()> {
        if !mount_point.exists() {
            let _ = std::fs::create_dir_all(mount_point);
        }

        let url = format!("http://avicull:{auth_token}@127.0.0.1:{port}/");
        let status = std::process::Command::new("/sbin/mount_webdav")
            .arg("-s")
            .arg("-o")
            .arg("ro")
            .arg(&url)
            .arg(mount_point)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "mount_webdav exited with status {status}"
            )))
        }
    }

    /// Unmounts a mounted virtual volume using `diskutil`.
    ///
    /// # Arguments
    /// * `mount_point` - Target directory path to unmount.
    ///
    /// # Errors
    /// Returns an error if diskutil execution fails.
    #[cfg(target_os = "macos")]
    pub fn unmount_darwin(mount_point: &std::path::Path) -> std::io::Result<()> {
        let status = std::process::Command::new("/usr/sbin/diskutil")
            .arg("unmount")
            .arg(mount_point)
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => {
                // Force unmount if regular unmount failed (e.g. busy)
                let force_status = std::process::Command::new("/usr/sbin/diskutil")
                    .arg("unmount")
                    .arg("force")
                    .arg(mount_point)
                    .status()?;
                if force_status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(format!(
                        "diskutil unmount force exited with status {force_status}"
                    )))
                }
            }
            Err(e) => Err(e),
        }
    }
}
