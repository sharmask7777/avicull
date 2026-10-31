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

    if options.target_dir.exists() && options.force {
        fs::remove_dir_all(&options.target_dir).with_context(|| {
            format!(
                "Failed to clean target directory '{}'",
                options.target_dir.display()
            )
        })?;
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
