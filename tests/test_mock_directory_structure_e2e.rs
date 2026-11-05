#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::pedantic,
    clippy::redundant_clone
)]

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
/// searches recursively within `dir`, skipping `.avicull` metadata directories.
fn find_linked_file(dir: &Path, stem: &str, ext: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current_dir) = stack.pop() {
        let entries = match fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            let lossy = file_name.to_string_lossy();

            if lossy.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                stack.push(path);
            } else if lossy.starts_with(stem) && lossy.ends_with(ext) {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn test_mock_directory_structure_full_organize_and_hardlink_integrity() {
    let local_fixtures_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/local_user_samples");

    if !local_fixtures_dir.exists() {
        eprintln!(
            "Notice: Mock directory structure fixtures not present at {:?}. Skipping test.",
            local_fixtures_dir
        );
        return;
    }

    let temp_root = tempdir().expect("create isolated temp directory");
    let input_dir = temp_root.path().join("User_Photos");
    let output_dir = input_dir.join("birds_hierarchy");

    copy_dir_all(&local_fixtures_dir, &input_dir).expect("failed to copy fixtures to test dir");

    // Inspect source non-avian assets (squirrel, butterfly, scenery) from fixtures
    let source_ranthambore = input_dir.join(
        "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg",
    );
    assert!(
        source_ranthambore.exists(),
        "Source photo must be preserved immutably"
    );

    let squirrel_photo = input_dir.join(
        "western ghats oct 2025/processed/1X7A9001_Mammalia_Malabar_Giant_Squirrel_20251002_Western_Ghats.jpg",
    );
    let squirrel_xmp = input_dir.join(
        "western ghats oct 2025/processed/1X7A9001_Mammalia_Malabar_Giant_Squirrel_20251002_Western_Ghats.jpg.xmp",
    );
    assert!(
        squirrel_photo.exists(),
        "Squirrel fixture photo must exist in copy"
    );
    let src_ino_squirrel = fs::metadata(&squirrel_photo).expect("stat squirrel").ino();
    let src_ino_squirrel_xmp = fs::metadata(&squirrel_xmp)
        .expect("stat squirrel xmp")
        .ino();

    let butterfly_photo = input_dir.join(
        "western ghats oct 2025/processed/1X7A9002_Insecta_Peacock_Butterfly_20251002_Western_Ghats.jpg",
    );
    let butterfly_xmp = input_dir.join(
        "western ghats oct 2025/processed/1X7A9002_Insecta_Peacock_Butterfly_20251002_Western_Ghats.jpg.xmp",
    );
    assert!(
        butterfly_photo.exists(),
        "Butterfly fixture photo must exist in copy"
    );
    let src_ino_butterfly = fs::metadata(&butterfly_photo)
        .expect("stat butterfly")
        .ino();
    let src_ino_butterfly_xmp = fs::metadata(&butterfly_xmp)
        .expect("stat butterfly xmp")
        .ino();

    let scenery_photo =
        input_dir.join("ladakh 2025/scenery/1X7A9003_Scenery_Pangong_Tso_20250915_Ladakh.jpg");
    let scenery_xmp =
        input_dir.join("ladakh 2025/scenery/1X7A9003_Scenery_Pangong_Tso_20250915_Ladakh.jpg.xmp");
    assert!(
        scenery_photo.exists(),
        "Scenery fixture photo must exist in copy"
    );
    let src_ino_scenery = fs::metadata(&scenery_photo).expect("stat scenery").ino();
    let src_ino_scenery_xmp = fs::metadata(&scenery_xmp).expect("stat scenery xmp").ino();

    let bin_path =
        std::env::var("AVICULL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_avicull").to_string());
    println!(
        "Executing Mock Directory Structure E2E test with binary: {}",
        bin_path
    );

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

    // 2. Correct non-avian assets with explicit multi-taxa categories and subjects
    let correct_mammal = std::process::Command::new(&bin_path)
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "1X7A9001",
            "--category",
            "Mammalia",
            "--species",
            "Malabar Giant Squirrel",
        ])
        .output()
        .expect("run avicull correct for Mammalia");
    assert!(
        correct_mammal.status.success(),
        "avicull correct Mammalia failed: {}",
        String::from_utf8_lossy(&correct_mammal.stderr)
    );

    let correct_insect = std::process::Command::new(&bin_path)
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "1X7A9002",
            "--category",
            "Insecta",
            "--species",
            "Peacock Butterfly",
        ])
        .output()
        .expect("run avicull correct for Insecta");
    assert!(
        correct_insect.status.success(),
        "avicull correct Insecta failed: {}",
        String::from_utf8_lossy(&correct_insect.stderr)
    );

    let correct_scenery = std::process::Command::new(&bin_path)
        .args([
            "correct",
            "--input",
            input_dir.to_str().unwrap(),
            "--file",
            "1X7A9003",
            "--category",
            "Scenery",
            "--species",
            "Mountain Landscape",
        ])
        .output()
        .expect("run avicull correct for Scenery");
    assert!(
        correct_scenery.status.success(),
        "avicull correct Scenery failed: {}",
        String::from_utf8_lossy(&correct_scenery.stderr)
    );

    // Ensure at least one confirmed bird exists under Bird/
    let linked_bird = find_linked_file(&output_dir.join("Bird"), "1X7A7827", ".jpg")
        .or_else(|| {
            let correct_bird = std::process::Command::new(&bin_path)
                .args([
                    "correct",
                    "--input",
                    input_dir.to_str().unwrap(),
                    "--file",
                    "1X7A7827",
                    "--category",
                    "Bird",
                    "--species",
                    "Rose-ringed Parakeet",
                ])
                .output()
                .expect("run avicull correct for Bird");
            assert!(correct_bird.status.success(), "correct Bird failed");
            find_linked_file(&output_dir.join("Bird"), "1X7A7827", ".jpg")
        })
        .expect("Expected hard linked bird photo under Bird/");

    // 3. Verify APFS zero-byte hardlink integrity across all 4 categories: Bird, Mammalia, Insecta, Scenery
    // Category 1: Bird
    let bird_source = input_dir.join(
        "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg",
    );
    let bird_source_xmp = input_dir.join(
        "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg.xmp",
    );
    let src_meta_bird = fs::metadata(&bird_source).expect("stat source bird photo");
    let dst_meta_bird = fs::metadata(&linked_bird).expect("stat dest bird photo");
    assert_eq!(
        dst_meta_bird.ino(),
        src_meta_bird.ino(),
        "Bird hard link inode must match source photo"
    );
    assert!(
        dst_meta_bird.nlink() >= 2,
        "Bird hard link count must be >= 2"
    );
    let bird_xmp = find_linked_file(linked_bird.parent().unwrap(), "1X7A7827", ".xmp")
        .expect("Bird companion .xmp sidecar must be linked");
    let src_meta_bird_xmp = fs::metadata(&bird_source_xmp).expect("stat source bird xmp");
    let dst_meta_bird_xmp = fs::metadata(&bird_xmp).expect("stat dest bird xmp");
    assert_eq!(
        dst_meta_bird_xmp.ino(),
        src_meta_bird_xmp.ino(),
        "Bird sidecar hard link inode must match source sidecar"
    );
    assert!(
        dst_meta_bird_xmp.nlink() >= 2,
        "Bird sidecar hard link count must be >= 2"
    );

    // Category 2: Mammalia
    let linked_mammal = find_linked_file(&output_dir.join("Mammalia"), "1X7A9001", ".jpg")
        .expect("Expected hard linked mammal photo under Mammalia/");
    let dst_meta_mammal = fs::metadata(&linked_mammal).expect("stat dest mammal photo");
    assert_eq!(
        dst_meta_mammal.ino(),
        src_ino_squirrel,
        "Mammalia hard link inode must match source photo"
    );
    assert!(
        dst_meta_mammal.nlink() >= 2,
        "Mammalia hard link count must be >= 2"
    );
    let mammal_xmp = find_linked_file(linked_mammal.parent().unwrap(), "1X7A9001", ".xmp")
        .expect("Mammalia companion .xmp sidecar must be linked");
    let dst_meta_mammal_xmp = fs::metadata(&mammal_xmp).expect("stat dest mammal xmp");
    assert_eq!(
        dst_meta_mammal_xmp.ino(),
        src_ino_squirrel_xmp,
        "Mammalia sidecar hard link inode must match source sidecar"
    );
    assert!(
        dst_meta_mammal_xmp.nlink() >= 2,
        "Mammalia sidecar hard link count must be >= 2"
    );

    // Category 3: Insecta
    let linked_insect = find_linked_file(&output_dir.join("Insecta"), "1X7A9002", ".jpg")
        .expect("Expected hard linked insect photo under Insecta/");
    let dst_meta_insect = fs::metadata(&linked_insect).expect("stat dest insect photo");
    assert_eq!(
        dst_meta_insect.ino(),
        src_ino_butterfly,
        "Insecta hard link inode must match source photo"
    );
    assert!(
        dst_meta_insect.nlink() >= 2,
        "Insecta hard link count must be >= 2"
    );
    let insect_xmp = find_linked_file(linked_insect.parent().unwrap(), "1X7A9002", ".xmp")
        .expect("Insecta companion .xmp sidecar must be linked");
    let dst_meta_insect_xmp = fs::metadata(&insect_xmp).expect("stat dest insect xmp");
    assert_eq!(
        dst_meta_insect_xmp.ino(),
        src_ino_butterfly_xmp,
        "Insecta sidecar hard link inode must match source sidecar"
    );
    assert!(
        dst_meta_insect_xmp.nlink() >= 2,
        "Insecta sidecar hard link count must be >= 2"
    );

    // Category 4: Scenery
    let linked_scenery = find_linked_file(&output_dir.join("Scenery"), "1X7A9003", ".jpg")
        .expect("Expected hard linked scenery photo under Scenery/");
    let dst_meta_scenery = fs::metadata(&linked_scenery).expect("stat dest scenery photo");
    assert_eq!(
        dst_meta_scenery.ino(),
        src_ino_scenery,
        "Scenery hard link inode must match source photo"
    );
    assert!(
        dst_meta_scenery.nlink() >= 2,
        "Scenery hard link count must be >= 2"
    );
    let scenery_xmp = find_linked_file(linked_scenery.parent().unwrap(), "1X7A9003", ".xmp")
        .expect("Scenery companion .xmp sidecar must be linked");
    let dst_meta_scenery_xmp = fs::metadata(&scenery_xmp).expect("stat dest scenery xmp");
    assert_eq!(
        dst_meta_scenery_xmp.ino(),
        src_ino_scenery_xmp,
        "Scenery sidecar hard link inode must match source sidecar"
    );
    assert!(
        dst_meta_scenery_xmp.nlink() >= 2,
        "Scenery sidecar hard link count must be >= 2"
    );

    // Also verify unclassified or needs review source photo is preserved
    let src_meta_ranthambore = fs::metadata(&source_ranthambore).expect("stat source photo");
    let linked_bulbul = find_linked_file(&output_dir, "1X7A0806", ".jpg")
        .expect("Expected hard linked bulbul photo in hierarchy output");
    let dst_meta_bulbul = fs::metadata(&linked_bulbul).expect("stat dest photo");
    assert_eq!(
        dst_meta_bulbul.ino(),
        src_meta_ranthambore.ino(),
        "APFS zero-byte hard link inode must match exactly"
    );
    assert!(
        dst_meta_bulbul.nlink() >= 2,
        "Hard link count must be at least 2"
    );

    // Verify HTML catalog was generated
    let catalog_html = output_dir.join("bird_catalog.html");
    assert!(
        catalog_html.exists(),
        "bird_catalog.html must be generated in output directory"
    );
    assert!(
        output_dir.join("catalog.html").exists(),
        "catalog.html alias must exist"
    );
    let catalog_content = fs::read_to_string(&catalog_html).expect("read catalog html");
    assert!(
        catalog_content.contains("<!DOCTYPE html>"),
        "Catalog must be valid HTML"
    );
    assert!(
        catalog_content.contains("Mammalia"),
        "Catalog must reflect Mammalia category filter"
    );
    assert!(
        catalog_content.contains("Insecta"),
        "Catalog must reflect Insecta category filter"
    );
    assert!(
        catalog_content.contains("Scenery"),
        "Catalog must reflect Scenery category filter"
    );

    // 4. Second delta run — must be fast and report 0 new bundles
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

    // Re-verify multi-taxa hardlinks remain intact after delta run
    assert!(find_linked_file(&output_dir.join("Bird"), "1X7A7827", ".jpg").is_some());
    assert!(find_linked_file(&output_dir.join("Mammalia"), "1X7A9001", ".jpg").is_some());
    assert!(find_linked_file(&output_dir.join("Insecta"), "1X7A9002", ".jpg").is_some());
    assert!(find_linked_file(&output_dir.join("Scenery"), "1X7A9003", ".jpg").is_some());

    // 5. Manifest DB and visual embeddings verification
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

    // Verify 512-dim visual embedding BLOB persistence
    let mut emb_stmt = conn
        .prepare(
            "SELECT category, visual_embedding FROM bundle_decisions WHERE visual_embedding IS NOT NULL",
        )
        .expect("prepare visual embedding query");

    let mut categories_with_embeddings = std::collections::HashSet::new();
    let mut total_embeddings = 0;

    let mut rows = emb_stmt.query([]).expect("query visual embeddings");
    while let Some(row) = rows.next().expect("fetch next embedding row") {
        let category: String = row.get(0).expect("get category column");
        let blob: Vec<u8> = row.get(1).expect("get visual_embedding blob");

        // 512-dim embedding of 32-bit floats is exactly 512 * 4 = 2048 bytes
        assert_eq!(
            blob.len(),
            2048,
            "Visual embedding BLOB must be exactly 2048 bytes for 512-dim f32 vector"
        );

        let floats: Vec<f32> = blob
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_ne_bytes(*chunk))
            .collect();
        assert_eq!(
            floats.len(),
            512,
            "Must unpack into exactly 512 f32 dimensions"
        );

        // Non-empty embedding: at least one non-zero component (L2 norm squared > 0)
        let norm_sq: f32 = floats.iter().map(|v| v * v).sum();
        assert!(
            norm_sq > 0.0,
            "Visual embedding vector must have non-zero magnitude"
        );

        categories_with_embeddings.insert(category);
        total_embeddings += 1;
    }

    assert!(
        total_embeddings > 0,
        "Manifest must contain non-empty visual embeddings"
    );
    assert!(
        categories_with_embeddings.contains("Bird"),
        "Manifest embeddings must include Bird category"
    );
    assert!(
        categories_with_embeddings.contains("Mammalia"),
        "Manifest embeddings must include Mammalia category"
    );
    assert!(
        categories_with_embeddings.contains("Insecta"),
        "Manifest embeddings must include Insecta category"
    );
    assert!(
        categories_with_embeddings.contains("Scenery"),
        "Manifest embeddings must include Scenery category"
    );
}
