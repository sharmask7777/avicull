use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const GITHUB_REPO: &str = "sharmask7777/avicull";
const DEFAULT_CHECK_INTERVAL_SECS: u64 = 86400; // 24 hours

/// Parses a semantic version string (e.g. "v0.2.1", "0.2.1", "V1.0.0-rc1") into (major, minor, patch).
pub fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    // Separate numeric portion from optional pre-release suffix (e.g. "1.0.0-rc1")
    let numeric = clean.split('-').next().unwrap_or("");
    let parts: Vec<&str> = numeric.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts[2].parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Returns true if candidate is strictly newer than current according to semver ordering.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some((cur_maj, cur_min, cur_pat)), Some((can_maj, can_min, can_pat))) => {
            if can_maj > cur_maj {
                return true;
            }
            if can_maj < cur_maj {
                return false;
            }
            if can_min > cur_min {
                return true;
            }
            if can_min < cur_min {
                return false;
            }
            can_pat > cur_pat
        }
        _ => false,
    }
}

/// Cached result of the periodic update check stored in user's cache directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckCache {
    pub last_checked_timestamp_secs: u64,
    pub latest_version: String,
    pub download_url: Option<String>,
}

impl UpdateCheckCache {
    /// Determines whether the cache interval has elapsed and a new check should be performed.
    pub fn should_check(&self, interval_secs: u64, current_timestamp_secs: u64) -> bool {
        current_timestamp_secs.saturating_sub(self.last_checked_timestamp_secs) >= interval_secs
    }

    /// Loads the update check cache from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read update cache from {}", path.display()))?;
        let cache: Self = serde_json::from_str(&content).with_context(|| {
            format!("Failed to parse update cache JSON from {}", path.display())
        })?;
        Ok(cache)
    }

    /// Persists the update check cache to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// Computes and verifies the SHA-256 checksum of a file on disk against an expected hexadecimal string.
pub fn verify_sha256_checksum(path: &Path, expected_hex: &str) -> Result<bool> {
    let output = std::process::Command::new("shasum")
        .arg("-a")
        .arg("256")
        .arg(path)
        .output()
        .context("Failed to execute shasum for checksum verification")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let calculated = stdout.split_whitespace().next().unwrap_or("");
    Ok(calculated.eq_ignore_ascii_case(expected_hex.trim()))
}

/// Atomically replaces a target executable binary with a staged new binary file.
pub fn atomic_replace_binary(target_bin: &Path, new_bin: &Path) -> Result<()> {
    let parent = target_bin.parent().unwrap_or_else(|| Path::new("."));
    let temp_dst = parent.join(format!(
        ".{}.tmp.{}",
        target_bin
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("avicull"),
        std::process::id()
    ));

    fs::copy(new_bin, &temp_dst).with_context(|| {
        format!(
            "Failed to copy binary to temporary staging path {}",
            temp_dst.display()
        )
    })?;

    let mut perms = fs::metadata(&temp_dst)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&temp_dst, perms)?;

    if let Err(e) = fs::rename(&temp_dst, target_bin) {
        let _ = fs::remove_file(&temp_dst);
        return Err(e).with_context(|| {
            format!(
                "Failed to atomically replace binary at {}",
                target_bin.display()
            )
        });
    }

    Ok(())
}

/// Metadata about a published GitHub release.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub tarball_url: String,
    pub sha256_url: String,
}

/// Fetches the latest published release information from the GitHub Releases API.
pub fn fetch_latest_release(repo: &str, timeout_secs: u32) -> Result<ReleaseInfo> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            &timeout_secs.to_string(),
            "-H",
            "User-Agent: avicull",
            "-H",
            "Accept: application/vnd.github.v3+json",
            &api_url,
        ])
        .output()
        .context("Failed to query GitHub releases API via curl")?;

    if !output.status.success() {
        return Err(anyhow!(
            "GitHub releases API returned error exit code: {}",
            output.status
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse GitHub releases response: {}", stdout))?;

    let tag_name = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("tag_name not found in release response"))?
        .to_string();

    let tarball_url = format!(
        "https://github.com/{}/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz",
        repo
    );
    let sha256_url = format!(
        "https://github.com/{}/releases/latest/download/avicull-aarch64-apple-darwin.tar.gz.sha256",
        repo
    );

    Ok(ReleaseInfo {
        tag_name,
        tarball_url,
        sha256_url,
    })
}

/// Resolves the default path to the update check cache file.
pub fn default_cache_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".cache/avicull/update_check.json"))
}

/// Checks for newer releases and returns a user-facing notice string if a new version is available.
/// Runs quickly with a short timeout and cached results so standard CLI execution is never degraded.
pub fn check_for_update_notice(current_version: &str) -> Option<String> {
    if std::env::var("AVICULL_NO_UPDATE_CHECK").is_ok() {
        return None;
    }

    let cache_path = default_cache_path()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

    let mut cache =
        UpdateCheckCache::load_from_file(&cache_path).unwrap_or_else(|_| UpdateCheckCache {
            last_checked_timestamp_secs: 0,
            latest_version: current_version.to_string(),
            download_url: None,
        });

    if cache.should_check(DEFAULT_CHECK_INTERVAL_SECS, now) {
        // Fast 1.5-second timeout to avoid perceptible latency on offline or slow connections.
        // Always update the timestamp even on network failure so offline users do not incur
        // a repeated 2-second curl timeout on every CLI invocation until connectivity returns.
        cache.last_checked_timestamp_secs = now;
        if let Ok(rel) = fetch_latest_release(GITHUB_REPO, 2) {
            cache.latest_version = rel.tag_name;
            cache.download_url = Some(rel.tarball_url);
        }
        let _ = cache.save_to_file(&cache_path);
    }

    if is_newer_version(current_version, &cache.latest_version) {
        Some(format!(
            "[avicull] Note: A newer version of avicull is available (v{} -> {}). Run 'avicull update' to upgrade.",
            current_version.trim_start_matches('v'),
            cache.latest_version
        ))
    } else {
        None
    }
}

/// Executes a self-update by downloading, checksum-verifying, and atomically installing the latest release.
pub fn perform_self_update(check_only: bool) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("[avicull] Checking for updates against GitHub Releases ({GITHUB_REPO})...");

    let release = fetch_latest_release(GITHUB_REPO, 10)
        .context("Failed to retrieve latest release information from GitHub")?;

    let is_newer = is_newer_version(current_version, &release.tag_name);

    if !is_newer {
        println!(
            "[avicull] ✓ avicull is already up to date (current: v{}, latest: {}).",
            current_version, release.tag_name
        );
        return Ok(());
    }

    println!(
        "[avicull] Newer release found: v{} -> {}",
        current_version, release.tag_name
    );

    if check_only {
        println!("[avicull] Check mode enabled: skipping binary download and installation.");
        return Ok(());
    }

    let current_exe = std::env::current_exe()
        .context("Failed to determine path to currently running avicull binary")?;

    println!(
        "[avicull] Downloading {} and checksum...",
        release.tarball_url
    );
    let tmp = tempfile::tempdir().context("Failed to create temporary directory for update")?;
    let tarball_path = tmp.path().join("avicull-latest.tar.gz");
    let sha256_path = tmp.path().join("avicull-latest.tar.gz.sha256");

    let curl_tar = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "60",
            "-o",
            tarball_path.to_str().unwrap(),
            &release.tarball_url,
        ])
        .output()
        .context("Failed to download release tarball")?;

    if !curl_tar.status.success() {
        return Err(anyhow!("Failed to download binary tarball from GitHub"));
    }

    let curl_sha = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "30",
            "-o",
            sha256_path.to_str().unwrap(),
            &release.sha256_url,
        ])
        .output()
        .context("Failed to download release checksum")?;

    if !curl_sha.status.success() {
        return Err(anyhow!("Failed to download sha256 checksum from GitHub"));
    }

    let sha_content =
        fs::read_to_string(&sha256_path).context("Failed to read downloaded checksum file")?;
    let expected_hash = sha_content
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("Malformed sha256 file format"))?;

    println!("[avicull] Verifying SHA-256 checksum ({expected_hash})...");
    if !verify_sha256_checksum(&tarball_path, expected_hash)? {
        return Err(anyhow!(
            "Checksum mismatch! The downloaded binary archive may be corrupted or tampered with."
        ));
    }
    println!("[avicull] ✓ Checksum verified successfully.");

    println!("[avicull] Unpacking archive...");
    let unpack_status = std::process::Command::new("tar")
        .args([
            "-xzf",
            tarball_path.to_str().unwrap(),
            "-C",
            tmp.path().to_str().unwrap(),
        ])
        .status()
        .context("Failed to execute tar command to unpack archive")?;

    if !unpack_status.success() {
        return Err(anyhow!("Failed to extract release archive"));
    }

    // Locate the newly unpacked binary in the temp directory
    let mut candidate_bin = None;
    for entry in walkdir::WalkDir::new(tmp.path()).into_iter().flatten() {
        if entry.file_type().is_file() && entry.file_name() == "avicull" {
            candidate_bin = Some(entry.into_path());
            break;
        }
    }

    let new_binary = candidate_bin
        .ok_or_else(|| anyhow!("Could not find avicull executable inside extracted archive"))?;

    println!(
        "[avicull] Installing new binary to {}...",
        current_exe.display()
    );

    match atomic_replace_binary(&current_exe, &new_binary) {
        Ok(()) => {
            println!(
                "[avicull] ✓ Successfully updated avicull from v{} to {}!",
                current_version, release.tag_name
            );

            // Update local cache file with the new version
            if let Some(cache_path) = default_cache_path() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let cache = UpdateCheckCache {
                    last_checked_timestamp_secs: now,
                    latest_version: release.tag_name.clone(),
                    download_url: Some(release.tarball_url),
                };
                let _ = cache.save_to_file(&cache_path);
            }
            Ok(())
        }
        Err(e) => {
            if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::PermissionDenied {
                    eprintln!(
                        "\n[avicull] Error: Permission denied writing to {}.",
                        current_exe.display()
                    );
                    eprintln!(
                        "[avicull] Please re-run the update command with administrative privileges:"
                    );
                    eprintln!("    sudo avicull update\n");
                    return Err(anyhow!(
                        "Permission denied updating binary in protected directory"
                    ));
                }
            }
            Err(e)
        }
    }
}
