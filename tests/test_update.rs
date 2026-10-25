use avicull::update::{
    atomic_replace_binary, default_cache_path, is_newer_version, parse_semver,
    verify_sha256_checksum, UpdateCheckCache,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

#[test]
fn test_parse_semver() {
    assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
    assert_eq!(parse_semver("v0.2.1"), Some((0, 2, 1)));
    assert_eq!(parse_semver("V1.0.0"), Some((1, 0, 0)));
    assert_eq!(parse_semver("v2.10.3-beta.1"), Some((2, 10, 3)));
    assert_eq!(parse_semver("invalid"), None);
    assert_eq!(parse_semver("1.0"), None);
    // Edge: plain zeros
    assert_eq!(parse_semver("0.0.0"), Some((0, 0, 0)));
    // Edge: large version numbers
    assert_eq!(parse_semver("100.200.300"), Some((100, 200, 300)));
}

#[test]
fn test_is_newer_version() {
    // Newer major
    assert!(is_newer_version("0.2.1", "1.0.0"));
    assert!(is_newer_version("v0.2.1", "v1.0.0"));

    // Newer minor
    assert!(is_newer_version("0.2.1", "0.3.0"));
    assert!(is_newer_version("0.2.1", "v0.3.0"));

    // Newer patch
    assert!(is_newer_version("0.2.1", "0.2.2"));
    assert!(is_newer_version("v0.2.1", "0.2.5"));

    // Same version
    assert!(!is_newer_version("0.2.1", "0.2.1"));
    assert!(!is_newer_version("v0.2.1", "0.2.1"));
    assert!(!is_newer_version("0.2.1", "v0.2.1"));

    // Older version
    assert!(!is_newer_version("0.2.1", "0.2.0"));
    assert!(!is_newer_version("0.2.1", "0.1.9"));
    assert!(!is_newer_version("1.0.0", "0.9.9"));

    // Unparseable versions never report as newer
    assert!(!is_newer_version("not-a-version", "also-not"));
    assert!(!is_newer_version("0.2.1", "invalid"));
}

#[test]
fn test_update_cache_expiry() {
    let now = 100000;
    let cache = UpdateCheckCache {
        last_checked_timestamp_secs: 90000,
        latest_version: "0.2.1".to_string(),
        download_url: None,
    };

    // 24-hour interval = 86400 seconds. 100000 - 90000 = 10000 (< 86400) -> should not check
    assert!(!cache.should_check(86400, now));

    // After 24 hours (now = 90000 + 86401 = 176401) -> should check
    assert!(cache.should_check(86400, 176401));

    // At exactly the boundary it should check
    assert!(cache.should_check(86400, 90000 + 86400));

    // Zero timestamp (fresh cache) should trigger a check once now exceeds the interval
    let fresh = UpdateCheckCache {
        last_checked_timestamp_secs: 0,
        latest_version: "0.2.2".to_string(),
        download_url: None,
    };
    // 86401s elapsed since epoch 0 - check is due
    assert!(fresh.should_check(86400, 86401));
    // Only 1000s elapsed since epoch 0 - not yet due
    assert!(!fresh.should_check(86400, 1000));
}

#[test]
fn test_update_cache_serialization_roundtrip() {
    let tmp = tempdir().unwrap();
    let cache_file = tmp.path().join("update_check.json");

    let cache = UpdateCheckCache {
        last_checked_timestamp_secs: 1700000000,
        latest_version: "v0.3.0".to_string(),
        download_url: Some("https://example.com/avicull.tar.gz".to_string()),
    };

    cache.save_to_file(&cache_file).unwrap();
    let loaded = UpdateCheckCache::load_from_file(&cache_file).unwrap();

    assert_eq!(
        loaded.last_checked_timestamp_secs,
        cache.last_checked_timestamp_secs
    );
    assert_eq!(loaded.latest_version, cache.latest_version);
    assert_eq!(loaded.download_url, cache.download_url);
}

#[test]
fn test_save_to_file_creates_parent_directories() {
    // save_to_file must create nested parent dirs that don't exist yet
    let tmp = tempdir().unwrap();
    let nested_path = tmp
        .path()
        .join("deeply")
        .join("nested")
        .join("cache")
        .join("update_check.json");

    let cache = UpdateCheckCache {
        last_checked_timestamp_secs: 42,
        latest_version: "0.1.0".to_string(),
        download_url: None,
    };

    cache.save_to_file(&nested_path).unwrap();
    assert!(nested_path.exists(), "Cache file must exist after save");
}

#[test]
fn test_load_from_file_missing_returns_error() {
    // Loading a nonexistent cache file must return Err, not panic
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("does_not_exist.json");
    assert!(UpdateCheckCache::load_from_file(&missing).is_err());
}

#[test]
fn test_default_cache_path_contains_avicull() {
    // default_cache_path() must resolve to a sensible path under $HOME
    if let Some(p) = default_cache_path() {
        let path_str = p.to_string_lossy();
        assert!(
            path_str.contains("avicull"),
            "Cache path must contain 'avicull': {}",
            path_str
        );
        assert!(
            path_str.contains(".cache"),
            "Cache path must be under .cache: {}",
            path_str
        );
    }
    // If HOME is not set the function returns None gracefully — no panic
}

#[test]
fn test_check_for_update_notice_suppressed_by_env_var() {
    // Verify that AVICULL_NO_UPDATE_CHECK=1 causes the function to return None immediately.
    // Tests modifying env vars must run serially to avoid parallel mutation races.
    // We verify the logic in isolation: the function reads the var at call time.
    // Run in a subprocess via the CLI to avoid parallel env mutation.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avicull"))
        .env("AVICULL_NO_UPDATE_CHECK", "1")
        .args(["help", "update"])
        .output()
        .expect("failed to run avicull with AVICULL_NO_UPDATE_CHECK=1");

    // The update notice is written to stderr; with opt-out env var, no notice should appear
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[avicull] Note: A newer version"),
        "Update notice must not appear when AVICULL_NO_UPDATE_CHECK=1 is set, got: {}",
        stderr
    );
}

#[test]
fn test_check_for_update_notice_no_spurious_notice_for_current_version() {
    // When installed version matches the cached latest, no notice should be emitted.
    // This is verified via is_newer_version returning false for equal versions.
    assert!(
        !is_newer_version(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION")),
        "Equal versions must never trigger an update notice"
    );
}

#[test]
fn test_verify_sha256_checksum() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("test_file.bin");
    fs::write(&file_path, b"hello avicull binary payload").unwrap();

    // echo -n "hello avicull binary payload" | shasum -a 256
    let expected_hash = "13c396684c6695e266145fc8be2aa86e0c128fae56ed36c165ebb032ffceb04f";

    assert!(verify_sha256_checksum(&file_path, expected_hash).unwrap());
    // Wrong hash must return false, not error
    assert!(!verify_sha256_checksum(
        &file_path,
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    )
    .unwrap());
    // Case-insensitive uppercase hash must also match
    assert!(verify_sha256_checksum(
        &file_path,
        "13C396684C6695E266145FC8BE2AA86E0C128FAE56ED36C165EBB032FFCEB04F"
    )
    .unwrap());
}

#[test]
fn test_atomic_replace_binary() {
    let tmp = tempdir().unwrap();
    let target_bin = tmp.path().join("current_app");
    fs::write(&target_bin, b"OLD_BINARY_V1").unwrap();
    let mut perms = fs::metadata(&target_bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&target_bin, perms).unwrap();

    let new_bin = tmp.path().join("extracted_new_app");
    fs::write(&new_bin, b"NEW_BINARY_V2").unwrap();

    atomic_replace_binary(&target_bin, &new_bin).unwrap();

    let updated_content = fs::read(&target_bin).unwrap();
    assert_eq!(updated_content, b"NEW_BINARY_V2");

    // Executable permission (0o755) must be preserved after atomic replace
    let updated_perms = fs::metadata(&target_bin).unwrap().permissions();
    assert_eq!(updated_perms.mode() & 0o777, 0o755);

    // Staging temp file must be cleaned up — no .tmp. files left in the directory
    let leftovers: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "Staging temp file must be cleaned up after atomic replace"
    );
}
