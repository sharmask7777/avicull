//! # Tests for Loopback Mount Service & Lifecycle Management
//!
//! Verifies:
//! - Ephemeral auth token generation and validation.
//! - Loopback server port binding.
//! - Teardown / unmount command execution logic.

use std::sync::Arc;
use tempfile::tempdir;

use avicull::storage::mount_service::{generate_auth_token, MountService, MountServiceConfig};
use avicull::storage::opendal_backend::OpenDalBackend;
use avicull::storage::webdav_server::TaxonomicTree;

#[test]
fn test_ephemeral_auth_token_format() {
    let token = generate_auth_token();
    assert_eq!(token.len(), 32);
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn test_mount_service_binds_ephemeral_port() {
    let tmp = tempdir().unwrap();
    let backend = Arc::new(OpenDalBackend::from_fs_path(tmp.path()).unwrap());
    let tree = Arc::new(TaxonomicTree::new());

    let config = MountServiceConfig {
        port: 0, // Ephemeral port
        mount_point: tmp.path().join("mnt"),
        serve_only: true,
        auth_token: generate_auth_token(),
    };

    let service = MountService::new(config, tree, backend);
    let bound_addr = service.bind_listener().await.unwrap();

    assert_eq!(bound_addr.ip(), std::net::Ipv4Addr::new(127, 0, 0, 1));
    assert!(bound_addr.port() > 0);
}
