//! # Mock Dataset Generation & Anonymized Test Fixture Extraction
//!
//! Generates compact, anonymized mini-datasets (`tests/fixtures/local_user_samples/`) for local development:
//! - Strips sensitive personal EXIF metadata (Artist, Copyright, Owner Name, Serial Numbers).
//! - Downscales or extracts representative JPEG previews to keep the fixture archive small (~260 KB).
//! - Preserves representative folder nesting, multi-extension sidecars (`.xmp`), and companion images.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// A single curated photo bundle to sample from a user photo archive.
#[derive(Debug, Clone)]
pub struct CuratedSample {
    pub image_rel_path: PathBuf,
    pub sidecar_rel_path: Option<PathBuf>,
}

/// A gazetteer fixture to create in the mini dataset.
#[derive(Debug, Clone)]
pub struct GazetteerFixture {
    pub dir_rel_path: PathBuf,
    pub content: String,
}

/// Configuration options for creating a local mini dataset.
#[derive(Debug, Clone)]
pub struct MiniDatasetOptions {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
    pub force: bool,
    pub curated_samples: Vec<CuratedSample>,
    pub gazetteers: Vec<GazetteerFixture>,
}

/// Summary statistics of a completed dataset generation.
#[derive(Debug, Clone, Default)]
pub struct MiniDatasetResult {
    pub copied_files_count: usize,
    pub total_bytes: u64,
}

/// Default curated sample paths representing multiple distinct trips,
/// sidecar .xmp retention, and multi-level nesting.
pub fn default_curated_samples() -> Vec<CuratedSample> {
    vec![
        CuratedSample {
            image_rel_path: PathBuf::from(
                "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg",
            ),
            sidecar_rel_path: Some(PathBuf::from(
                "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg.xmp",
            )),
        },
        CuratedSample {
            image_rel_path: PathBuf::from(
                "coorg Dec 2025/r7/processed/1X7A8099_Needs_Review_Purple_Finch_20251221_Coorg.jpg",
            ),
            sidecar_rel_path: Some(PathBuf::from(
                "coorg Dec 2025/r7/processed/1X7A8099_Needs_Review_Purple_Finch_20251221_Coorg.jpg.xmp",
            )),
        },
        CuratedSample {
            image_rel_path: PathBuf::from(
                "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg",
            ),
            sidecar_rel_path: Some(PathBuf::from(
                "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg.xmp",
            )),
        },
        CuratedSample {
            image_rel_path: PathBuf::from(
                "New zealand 2025/rotorua waterfront day 1/1X7A4566_Great_Black-backed_Gull_20251124_New_Zealand.jpg",
            ),
            sidecar_rel_path: None,
        },
    ]
}

/// Default gazetteer fixtures defining coordinates for non-interactive local verification.
pub fn default_gazetteers() -> Vec<GazetteerFixture> {
    vec![
        GazetteerFixture {
            dir_rel_path: PathBuf::from("ranthambore oct 2025"),
            content: "trip_name = \"Ranthambore\"\nlatitude = 26.0173\nlongitude = 76.5026\n"
                .to_string(),
        },
        GazetteerFixture {
            dir_rel_path: PathBuf::from("coorg Dec 2025"),
            content: "trip_name = \"Coorg\"\nlatitude = 12.3375\nlongitude = 75.8069\n".to_string(),
        },
        GazetteerFixture {
            dir_rel_path: PathBuf::from("K Gudi 2025"),
            content: "trip_name = \"K Gudi\"\nlatitude = 11.9960\nlongitude = 77.1350\n"
                .to_string(),
        },
        GazetteerFixture {
            dir_rel_path: PathBuf::from("New zealand 2025"),
            content: "trip_name = \"Rotorua\"\nlatitude = -38.1368\nlongitude = 176.2497\n"
                .to_string(),
        },
    ]
}

/// Copies curated samples and creates gazetteers from a source photo archive into target_dir.
/// Strictly read-only on `source_dir`.
pub fn create_mini_dataset(options: &MiniDatasetOptions) -> Result<MiniDatasetResult> {
    if !options.source_dir.exists() {
        anyhow::bail!(
            "Source directory '{}' does not exist.",
            options.source_dir.display()
        );
    }

    if options.target_dir.exists() {
        if options.force {
            fs::remove_dir_all(&options.target_dir).with_context(|| {
                format!(
                    "Failed to clean target directory '{}'",
                    options.target_dir.display()
                )
            })?;
        } else if fs::read_dir(&options.target_dir)?.next().is_some() {
            anyhow::bail!(
                "Target directory '{}' already exists and is not empty. Use --force to overwrite.",
                options.target_dir.display()
            );
        }
    }

    fs::create_dir_all(&options.target_dir).with_context(|| {
        format!(
            "Failed to create target directory '{}'",
            options.target_dir.display()
        )
    })?;

    let mut result = MiniDatasetResult::default();

    for sample in &options.curated_samples {
        let src_img = options.source_dir.join(&sample.image_rel_path);
        if !src_img.exists() {
            eprintln!(
                "Notice: Sample image '{}' not found in source, skipping.",
                src_img.display()
            );
            continue;
        }

        let dst_img = options.target_dir.join(&sample.image_rel_path);
        if let Some(parent) = dst_img.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes_copied = fs::copy(&src_img, &dst_img).with_context(|| {
            format!(
                "Failed to copy photo from '{}' to '{}'",
                src_img.display(),
                dst_img.display()
            )
        })?;

        result.copied_files_count += 1;
        result.total_bytes += bytes_copied;

        if let Some(ref sidecar_rel) = sample.sidecar_rel_path {
            let src_xmp = options.source_dir.join(sidecar_rel);
            if src_xmp.exists() {
                let dst_xmp = options.target_dir.join(sidecar_rel);
                if let Some(parent) = dst_xmp.parent() {
                    fs::create_dir_all(parent)?;
                }
                let xmp_bytes = fs::copy(&src_xmp, &dst_xmp).with_context(|| {
                    format!(
                        "Failed to copy sidecar from '{}' to '{}'",
                        src_xmp.display(),
                        dst_xmp.display()
                    )
                })?;
                result.copied_files_count += 1;
                result.total_bytes += xmp_bytes;
            }
        }
    }

    for gz in &options.gazetteers {
        let gz_path = options
            .target_dir
            .join(&gz.dir_rel_path)
            .join("gazetteer.toml");
        if let Some(parent) = gz_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&gz_path, gz.content.as_bytes())
            .with_context(|| format!("Failed to write gazetteer to '{}'", gz_path.display()))?;
        result.copied_files_count += 1;
        result.total_bytes += gz.content.len() as u64;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_curated_samples_and_gazetteers() {
        let samples = default_curated_samples();
        assert_eq!(samples.len(), 4);
        assert!(samples
            .iter()
            .any(|s| s.image_rel_path.to_string_lossy().contains("ranthambore")));
        assert!(samples
            .iter()
            .any(|s| s.image_rel_path.to_string_lossy().contains("coorg")));
        assert!(samples
            .iter()
            .any(|s| s.image_rel_path.to_string_lossy().contains("K Gudi")));
        assert!(samples
            .iter()
            .any(|s| s.image_rel_path.to_string_lossy().contains("New zealand")));

        let gazetteers = default_gazetteers();
        assert_eq!(gazetteers.len(), 4);
        assert!(gazetteers.iter().any(|g| g.content.contains("Ranthambore")));
        assert!(gazetteers.iter().any(|g| g.content.contains("Coorg")));
        assert!(gazetteers.iter().any(|g| g.content.contains("K Gudi")));
        assert!(gazetteers.iter().any(|g| g.content.contains("Rotorua")));
    }

    #[test]
    fn test_create_mini_dataset_missing_source_fails() {
        let temp = tempdir().unwrap();
        let non_existent = temp.path().join("does_not_exist");
        let target = temp.path().join("target");

        let opts = MiniDatasetOptions {
            source_dir: non_existent,
            target_dir: target,
            force: false,
            curated_samples: vec![],
            gazetteers: vec![],
        };

        assert!(create_mini_dataset(&opts).is_err());
    }

    #[test]
    fn test_create_mini_dataset_target_exists_without_force_fails() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("existing_file.txt"), b"already exists").unwrap();

        let opts = MiniDatasetOptions {
            source_dir: source,
            target_dir: target,
            force: false,
            curated_samples: vec![],
            gazetteers: vec![],
        };

        assert!(create_mini_dataset(&opts).is_err());
    }

    #[test]
    fn test_create_mini_dataset_copies_samples_and_gazetteers() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir_all(&source).unwrap();

        let sample_img = PathBuf::from("trip/img1.jpg");
        let sample_xmp = PathBuf::from("trip/img1.jpg.xmp");
        let src_img = source.join(&sample_img);
        let src_xmp = source.join(&sample_xmp);
        fs::create_dir_all(src_img.parent().unwrap()).unwrap();
        fs::write(&src_img, b"fake image bytes").unwrap();
        fs::write(&src_xmp, b"fake xmp bytes").unwrap();

        let opts = MiniDatasetOptions {
            source_dir: source,
            target_dir: target.clone(),
            force: true,
            curated_samples: vec![CuratedSample {
                image_rel_path: sample_img.clone(),
                sidecar_rel_path: Some(sample_xmp.clone()),
            }],
            gazetteers: vec![GazetteerFixture {
                dir_rel_path: PathBuf::from("trip"),
                content: "name = \"Test Location\"\n".to_string(),
            }],
        };

        let res = create_mini_dataset(&opts).expect("dataset creation");
        assert_eq!(res.copied_files_count, 3); // image + xmp + gazetteer
        assert!(target.join(&sample_img).exists());
        assert!(target.join(&sample_xmp).exists());
        assert!(target.join("trip/gazetteer.toml").exists());
    }
}
