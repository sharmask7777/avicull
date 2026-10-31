#![cfg(unix)]

use rusqlite::Connection;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

/// Recursively copies a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Locates a file whose name matches the format: `<stem>_<conf>.<ext>` or starts with `<stem>`
fn find_linked_file(dir: &Path, stem: &str, ext: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let lossy = name.to_string_lossy();
        if lossy.starts_with(stem) && lossy.ends_with(ext) {
            return Some(path);
        }
    }
    None
}

#[test]
fn test_local_user_samples_e2e_inner_loop() {
    let local_fixtures_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/local_user_samples");

    if !local_fixtures_dir.exists() {
        eprintln!(
            "Notice: Local user sample fixtures not present at {:?}. Skipping test.",
            local_fixtures_dir
        );
        return;
    }

    let temp_root = tempdir().expect("create isolated temp directory");
    let input_dir = temp_root.path().join("User_Photos");
    let output_dir = input_dir.join("birds_hierarchy");

    copy_dir_all(&local_fixtures_dir, &input_dir).expect("failed to copy fixtures to test dir");

    let bin_path =
        std::env::var("AVICULL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_avicull").to_string());
    println!("Executing Inner-Loop E2E test with binary: {}", bin_path);

    // 1. Initial full organization pass
    let output1 = std::process::Command::new(&bin_path)
        .arg("organize")
        .arg("--input")
        .arg(&input_dir)
        .arg("--output")
        .arg(&output_dir)
        .arg("--mode")
        .arg("hierarchy")
        .arg("--no-prompt")
        .output()
        .expect("failed to spawn avicull binary");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stderr1 = String::from_utf8_lossy(&output1.stderr);
    assert!(
        output1.status.success(),
        "avicull organize run 1 failed!\nSTDOUT:\n{}\nSTDERR:\n{}",
        stdout1,
        stderr1
    );

    // Verify APFS zero-byte hardlink integrity for bundles
    let source_ranthambore = input_dir.join(
        "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg",
    );
    assert!(
        source_ranthambore.exists(),
        "Source photo must be preserved immutably"
    );
    let src_meta = fs::metadata(&source_ranthambore).expect("stat source photo");
    let src_ino = src_meta.ino();

    let linked_bulbul = find_linked_file(
        &output_dir.join("_Needs_Review/Red_vented_Bulbul"),
        "1X7A0806",
        ".jpg",
    )
    .or_else(|| find_linked_file(&output_dir.join("Red_vented_Bulbul"), "1X7A0806", ".jpg"))
    .expect("Expected hard linked photo in hierarchy output");

    let dst_meta = fs::metadata(&linked_bulbul).expect("stat dest photo");
    assert_eq!(
        dst_meta.ino(),
        src_ino,
        "APFS zero-byte hard link inode must match exactly"
    );
    assert!(dst_meta.nlink() >= 2, "Hard link count must be at least 2");

    // Verify sidecar .xmp was linked alongside
    let linked_xmp = find_linked_file(linked_bulbul.parent().unwrap(), "1X7A0806", ".xmp")
        .expect("Expected bundled .xmp sidecar in hierarchy output");
    assert!(linked_xmp.exists(), "Sidecar must exist");

    // Verify HTML catalog was generated
    let catalog_html = output_dir.join("bird_catalog.html");
    assert!(
        catalog_html.exists(),
        "bird_catalog.html must be generated in output directory"
    );
    let catalog_content = fs::read_to_string(&catalog_html).expect("read catalog html");
    assert!(
        catalog_content.contains("<!DOCTYPE html>"),
        "Catalog must be valid HTML"
    );

    // 2. Second delta run — must be fast and report 0 new bundles
    let output2 = std::process::Command::new(&bin_path)
        .arg("organize")
        .arg("--input")
        .arg(&input_dir)
        .arg("--output")
        .arg(&output_dir)
        .arg("--mode")
        .arg("hierarchy")
        .arg("--no-prompt")
        .output()
        .expect("failed to spawn avicull binary on delta run");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(
        output2.status.success(),
        "avicull delta run failed!\nSTDOUT:\n{}",
        stdout2
    );
    assert!(
        stdout2.contains("0 new bundles detected to process")
            || stdout2.contains("already cached")
            || stdout2.contains("already up to date"),
        "Delta run must recognize cached bundles and not recompute them from scratch"
    );

    // 3. Manifest DB verification
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    assert!(db_path.exists(), "Manifest SQLite must exist");
    let conn = Connection::open(&db_path).expect("open manifest sqlite");
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM bundle_thumbnails")
        .expect("prepare thumbnail count query");
    let thumbnail_count: i64 = stmt
        .query_row([], |r| r.get(0))
        .expect("query thumbnail count");
    assert!(
        thumbnail_count > 0,
        "Manifest DB must contain cached thumbnails for fast delta re-runs"
    );
}
