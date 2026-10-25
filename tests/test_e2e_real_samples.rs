#![cfg(unix)]

use rusqlite::Connection;
use std::collections::HashMap;
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

/// Locates a file whose name matches the format: `<stem>_<conf>.<ext>`
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

struct ExpectedBundle<'a> {
    species_slug: &'a str,
    sample_stem: &'a str,
    rel_source_path: &'a str,
}

#[test]
fn test_e2e_real_samples_classification_and_hardlinks() {
    let temp_root = tempdir().expect("create isolated temp directory");
    let input_dir = temp_root.path().join("Raw_Trips");
    let output_dir = temp_root.path().join("birds_hierarchy");

    // 1. Setup isolated fixtures
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/e2e_samples");
    assert!(
        fixtures_dir.exists(),
        "Fixture samples directory must exist at {:?}",
        fixtures_dir
    );
    copy_dir_all(&fixtures_dir, &input_dir).expect("failed to copy fixtures to test dir");

    let bin_path =
        std::env::var("AVICULL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_avicull").to_string());
    println!("Executing E2E test with binary: {}", bin_path);

    // 2. Execute avicull CLI
    let output = std::process::Command::new(&bin_path)
        .arg("organize")
        .arg("--input")
        .arg(&input_dir)
        .arg("--output")
        .arg(&output_dir)
        .arg("--mode")
        .arg("hierarchy")
        .output()
        .expect("failed to spawn avicull binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "avicull organize failed!\nSTDOUT:\n{}\nSTDERR:\n{}",
        stdout,
        stderr
    );

    // 3. Expected Test Expectations Table
    let expectations = [
        ExpectedBundle {
            species_slug: "Chukar",
            sample_stem: "sample_chukar",
            rel_source_path: "leh/sample_chukar.jpg",
        },
        ExpectedBundle {
            species_slug: "Himalayan_Rubythroat",
            sample_stem: "sample_rubythroat",
            rel_source_path: "leh/sample_rubythroat.jpg",
        },
        ExpectedBundle {
            species_slug: "Black_Redstart",
            sample_stem: "sample_redstart",
            rel_source_path: "leh/sample_redstart.jpg",
        },
        ExpectedBundle {
            species_slug: "White_throated_Kingfisher",
            sample_stem: "sample_kingfisher",
            rel_source_path: "nagarhole/sample_kingfisher.jpg",
        },
        ExpectedBundle {
            species_slug: "Red_wattled_Lapwing",
            sample_stem: "sample_lapwing",
            rel_source_path: "nagarhole/sample_lapwing.jpg",
        },
    ];

    let models_available = Path::new("models/species_embeddings.safetensors").exists()
        || std::env::var("HOME")
            .ok()
            .map(|h| {
                PathBuf::from(h)
                    .join(".cache/avicull/models/species_embeddings.safetensors")
                    .exists()
            })
            .unwrap_or(false);

    if models_available {
        // 4. Verify Filesystem Hard Links & APFS Invariants for Confirmed Species
        for exp in &expectations {
            let species_dir = output_dir.join(exp.species_slug);
            assert!(
                species_dir.exists(),
                "Expected species directory {:?} to exist in output.\nOutput tree:\n{}",
                species_dir,
                stdout
            );

            let linked =
                find_linked_file(&species_dir, exp.sample_stem, ".jpg").unwrap_or_else(|| {
                    let existing: Vec<_> = fs::read_dir(&species_dir)
                        .map(|rd| {
                            rd.flatten()
                                .map(|e| e.file_name().to_string_lossy().to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    panic!(
                        "Could not find file with stem '{}' in {:?}. Files found: {:?}",
                        exp.sample_stem, species_dir, existing
                    )
                });

            let src_path = input_dir.join(exp.rel_source_path);
            let src_meta = fs::metadata(&src_path).expect("source metadata");
            let dst_meta = fs::metadata(&linked).expect("linked destination metadata");

            assert_eq!(
                src_meta.dev(),
                dst_meta.dev(),
                "Device IDs must match on same APFS volume for {}",
                exp.sample_stem
            );
            assert_eq!(
                src_meta.ino(),
                dst_meta.ino(),
                "Inodes must match for zero-byte hardlink of {}",
                exp.sample_stem
            );
            assert_eq!(
                dst_meta.nlink(),
                2,
                "st_nlink must be exactly 2 for {}",
                exp.sample_stem
            );
        }

        // 5. Verify SQLite Manifest Records & Strict Stem-to-Species Mapping
        let db_path = output_dir.join(".avicull/manifest.sqlite");
        assert!(
            db_path.exists(),
            "Manifest database must exist at {:?}",
            db_path
        );

        let conn = Connection::open(&db_path).expect("open sqlite db");
        let mut stmt = conn
            .prepare(
                "SELECT original_stem, assigned_species_slug, final_conf, routing_tier FROM bundles",
            )
            .expect("prepare select");

        let expected_map: HashMap<&str, &str> = expectations
            .iter()
            .map(|e| (e.sample_stem, e.species_slug))
            .collect();

        let mut rows = stmt.query([]).expect("query bundles");
        let mut verified_count = 0;

        while let Some(row) = rows.next().expect("next row") {
            verified_count += 1;
            let stem: String = row.get(0).expect("column original_stem");
            let slug: String = row.get(1).expect("column assigned_species_slug");
            let conf: f64 = row.get(2).expect("column final_conf");
            let tier: String = row.get(3).expect("column routing_tier");

            let expected_slug = expected_map
                .get(stem.as_str())
                .unwrap_or_else(|| panic!("Unexpected stem '{}' found in manifest database", stem));

            assert_eq!(
                &slug, expected_slug,
                "Bundle '{}' classified as '{}', expected '{}'",
                stem, slug, expected_slug
            );
            assert_eq!(
                tier, "ConfirmedSpecies",
                "Bundle '{}' must be ConfirmedSpecies",
                stem
            );
            assert!(
                conf >= 0.80,
                "Confidence for '{}' must be >= 0.80, got {:.3}",
                stem,
                conf
            );
        }

        assert_eq!(
            verified_count,
            expectations.len(),
            "Expected exactly {} bundles in manifest database",
            expectations.len()
        );
    } else {
        // Fallback verification when neural weights are absent in CI:
        // Verify that bundles are gracefully routed to _No_Bird_Detected while preserving APFS hardlink invariants
        println!("Neural weights not present; verifying APFS zero-byte hard links under _No_Bird_Detected");
        let fallback_dir = output_dir.join("_No_Bird_Detected");
        assert!(
            fallback_dir.exists(),
            "Expected _No_Bird_Detected directory {:?} to exist in output.\nOutput tree:\n{}",
            fallback_dir,
            stdout
        );

        for exp in &expectations {
            let linked =
                find_linked_file(&fallback_dir, exp.sample_stem, ".jpg").unwrap_or_else(|| {
                    let existing: Vec<_> = fs::read_dir(&fallback_dir)
                        .map(|rd| {
                            rd.flatten()
                                .map(|e| e.file_name().to_string_lossy().to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    panic!(
                        "Could not find file with stem '{}' in {:?}. Files found: {:?}",
                        exp.sample_stem, fallback_dir, existing
                    )
                });

            let src_path = input_dir.join(exp.rel_source_path);
            let src_meta = fs::metadata(&src_path).expect("source metadata");
            let dst_meta = fs::metadata(&linked).expect("linked destination metadata");

            assert_eq!(
                src_meta.dev(),
                dst_meta.dev(),
                "Device IDs must match on same APFS volume for {}",
                exp.sample_stem
            );
            assert_eq!(
                src_meta.ino(),
                dst_meta.ino(),
                "Inodes must match for zero-byte hardlink of {}",
                exp.sample_stem
            );
            assert_eq!(
                dst_meta.nlink(),
                2,
                "st_nlink must be exactly 2 for {}",
                exp.sample_stem
            );
        }

        let db_path = output_dir.join(".avicull/manifest.sqlite");
        assert!(
            db_path.exists(),
            "Manifest database must exist at {:?}",
            db_path
        );

        let conn = Connection::open(&db_path).expect("open sqlite db");
        let mut stmt = conn
            .prepare("SELECT original_stem, routing_tier FROM bundles")
            .expect("prepare select");

        let mut rows = stmt.query([]).expect("query bundles");
        let mut verified_count = 0;

        while let Some(row) = rows.next().expect("next row") {
            verified_count += 1;
            let _stem: String = row.get(0).expect("column original_stem");
            let tier: String = row.get(1).expect("column routing_tier");
            assert_eq!(tier, "NoBirdDetected");
        }

        assert_eq!(
            verified_count,
            expectations.len(),
            "Expected exactly {} bundles in manifest database",
            expectations.len()
        );
    }
}
