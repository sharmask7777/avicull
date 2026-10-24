use crate::types::AssetBundle;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const RAW_EXTS: &[&str] = &[
    "arw", "cr2", "cr3", "nef", "orf", "rw2", "dng", "pef", "srw", "raf",
];
const JPEG_EXTS: &[&str] = &["jpg", "jpeg"];
const HEIC_EXTS: &[&str] = &["heic", "heif"];
const PNG_EXTS: &[&str] = &["png"];
const TIFF_EXTS: &[&str] = &["tif", "tiff"];
const SIDECAR_EXTS: &[&str] = &["xmp", "dop", "pp3", "acr", "on1"];

/// Validates that input and output directories are on the same filesystem volume
/// and neither path is an ancestor/nested within the other.
pub fn validate_volume_and_disjointness(input: &Path, output: &Path) -> Result<()> {
    if !input.exists() {
        return Err(anyhow!(
            "Input directory does not exist: {}",
            input.display()
        ));
    }

    if !output.exists() {
        fs::create_dir_all(output)
            .with_context(|| format!("Failed to create output directory: {}", output.display()))?;
    }

    let can_input = input
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize input path: {}", input.display()))?;
    let can_output = output
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize output path: {}", output.display()))?;

    // Disjointness check
    if can_output.starts_with(&can_input) {
        return Err(crate::types::AvicullError::NestedPathDisjointness {
            nested: can_output,
            parent: can_input,
        }
        .into());
    }
    if can_input.starts_with(&can_output) {
        return Err(crate::types::AvicullError::NestedPathDisjointness {
            nested: can_input,
            parent: can_output,
        }
        .into());
    }

    // Volume (st_dev) check
    let in_meta = fs::metadata(&can_input)
        .with_context(|| format!("Failed to stat input directory: {}", can_input.display()))?;
    let out_meta = fs::metadata(&can_output)
        .with_context(|| format!("Failed to stat output directory: {}", can_output.display()))?;

    if in_meta.dev() != out_meta.dev() {
        return Err(crate::types::AvicullError::VolumeMismatch {
            input_dev: in_meta.dev(),
            output_dev: out_meta.dev(),
        }
        .into());
    }

    Ok(())
}

fn image_priority(ext: &str) -> Option<u8> {
    let lower = ext.to_lowercase();
    if RAW_EXTS.contains(&lower.as_str()) {
        Some(5) // Highest priority
    } else if JPEG_EXTS.contains(&lower.as_str()) {
        Some(4)
    } else if HEIC_EXTS.contains(&lower.as_str()) {
        Some(3)
    } else if PNG_EXTS.contains(&lower.as_str()) {
        Some(2)
    } else if TIFF_EXTS.contains(&lower.as_str()) {
        Some(1)
    } else {
        None
    }
}

fn is_sidecar(ext: &str) -> bool {
    SIDECAR_EXTS.contains(&ext.to_lowercase().as_str())
}

/// Decomposes a filename into its base stem and sidecar/image status.
/// Handles compound sidecars like `DSC_0001.ARW.xmp` or `DSC_0001.JPG.XMP`.
pub fn decompose_filename(filename: &str) -> Option<(String, bool)> {
    let path = Path::new(filename);
    let ext1 = path.extension()?.to_str()?;

    if is_sidecar(ext1) {
        // May be a compound sidecar like DSC_0001.ARW.xmp
        let stem_path = Path::new(path.file_stem()?.to_str()?);
        if let Some(ext2) = stem_path.extension().and_then(|e| e.to_str()) {
            if image_priority(ext2).is_some() {
                // Compound sidecar! Strip image ext to reach root stem
                let root_stem = stem_path.file_stem()?.to_str()?.to_string();
                return Some((root_stem, true));
            }
        }
        let root_stem = path.file_stem()?.to_str()?.to_string();
        return Some((root_stem, true));
    }

    if image_priority(ext1).is_some() {
        let root_stem = path.file_stem()?.to_str()?.to_string();
        return Some((root_stem, false));
    }

    None
}

/// Scans the input tree recursively and groups related photo and sidecar files into `AssetBundle`s.
pub fn scan_input_tree(input_dir: &Path) -> Result<Vec<AssetBundle>> {
    let can_input = input_dir.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize input directory: {}",
            input_dir.display()
        )
    })?;

    // Group files by (canonical_dir, stem_lowercase)
    struct GroupAcc {
        trip_name: String,
        original_stem: String,
        images: Vec<(PathBuf, u8)>, // (path, priority)
        sidecars: Vec<PathBuf>,
    }

    let mut groups: HashMap<(PathBuf, String), GroupAcc> = HashMap::new();

    let walker = WalkDir::new(&can_input)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') || name == "@eaDir" {
                return false;
            }
            true
        });

    for entry_res in walker {
        let entry = entry_res?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();
        let file_name = entry.file_name().to_string_lossy();

        if let Some((stem, is_sidecar_file)) = decompose_filename(&file_name) {
            let parent_dir = path
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf();
            let stem_lower = stem.to_lowercase();
            let key = (parent_dir.clone(), stem_lower);

            let trip_name = if parent_dir == can_input {
                can_input
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Root".to_string())
            } else {
                // Find top-level folder relative to can_input
                let rel = parent_dir.strip_prefix(&can_input).unwrap_or(&parent_dir);
                let first_component = rel
                    .components()
                    .next()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                first_component
            };

            let group = groups.entry(key).or_insert_with(|| GroupAcc {
                trip_name,
                original_stem: stem.clone(),
                images: Vec::new(),
                sidecars: Vec::new(),
            });

            if is_sidecar_file {
                group.sidecars.push(path);
            } else {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or_default();
                let prio = image_priority(ext).unwrap_or(0);
                group.images.push((path, prio));
            }
        }
    }

    let mut bundles = Vec::new();

    for (_, mut group) in groups {
        if group.images.is_empty() {
            // Stray sidecar without any companion or primary image
            continue;
        }

        // Sort images by priority descending
        group.images.sort_by(|a, b| b.1.cmp(&a.1));

        let primary_image = group.images[0].0.clone();
        let companion_images = group.images[1..].iter().map(|(p, _)| p.clone()).collect();

        let can_primary = primary_image
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize {}", primary_image.display()))?;
        let meta = fs::metadata(&can_primary)
            .with_context(|| format!("Failed to stat {}", can_primary.display()))?;

        let bundle_id = blake3::hash(can_primary.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        let source_inode = meta.ino();

        bundles.push(AssetBundle {
            bundle_id,
            source_inode,
            trip_folder_name: group.trip_name,
            original_stem: group.original_stem,
            primary_image,
            companion_images,
            sidecar_files: group.sidecars,
            capture_timestamp_ms: None,
            gps_coords: None,
        });
    }

    // Sort bundles deterministically by primary_image path
    bundles.sort_by(|a, b| a.primary_image.cmp(&b.primary_image));

    Ok(bundles)
}

/// Scans raw bytes for embedded JPEG preview markers `0xFFD8`..`0xFFD9`
/// Extracts embedded full-size JPEG preview bytes from a RAW file.
/// Parses JPEG segment markers and lengths (skipping APP1 EXIF thumbnails)
/// and returns the largest complete preview stream found.
pub fn extract_embedded_jpeg_preview(raw_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut largest: Option<Vec<u8>> = None;
    let mut max_len = 0;

    let len = raw_bytes.len();
    if len < 4 {
        return None;
    }

    // Helper to find the end of a JPEG starting at an SOI offset
    let parse_jpeg_end = |start: usize| -> Option<usize> {
        if start + 2 >= len || raw_bytes[start] != 0xFF || raw_bytes[start + 1] != 0xD8 {
            return None;
        }

        let mut pos = start + 2;
        while pos + 1 < len {
            // Find next marker prefix 0xFF
            if raw_bytes[pos] != 0xFF {
                pos += 1;
                continue;
            }

            // Skip consecutive 0xFF fill bytes
            while pos + 1 < len && raw_bytes[pos + 1] == 0xFF {
                pos += 1;
            }
            if pos + 1 >= len {
                break;
            }

            let marker = raw_bytes[pos + 1];

            // EOI: End of Image
            if marker == 0xD9 {
                return Some(pos + 2);
            }

            // Standalone markers without payload
            if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x00 {
                pos += 2;
                continue;
            }

            // Start of Scan (SOS): scan data follows until EOI
            if marker == 0xDA {
                if pos + 3 >= len {
                    break;
                }
                let sos_len = ((raw_bytes[pos + 2] as usize) << 8) | (raw_bytes[pos + 3] as usize);
                if sos_len < 2 || pos + 2 + sos_len > len {
                    break;
                }
                pos += 2 + sos_len;

                // Scan entropy-coded data until EOI (0xFF, 0xD9)
                while pos + 1 < len {
                    if raw_bytes[pos] == 0xFF {
                        let m = raw_bytes[pos + 1];
                        if m == 0xD9 {
                            return Some(pos + 2);
                        } else if m != 0x00 && !(0xD0..=0xD7).contains(&m) {
                            // Marker in scan data
                            break;
                        }
                    }
                    pos += 1;
                }
                continue;
            }

            // Segment with 16-bit big-endian length
            if pos + 3 >= len {
                break;
            }
            let seg_len = ((raw_bytes[pos + 2] as usize) << 8) | (raw_bytes[pos + 3] as usize);
            if seg_len < 2 || pos + 2 + seg_len > len {
                break;
            }
            pos += 2 + seg_len;
        }

        None
    };

    // Scan for SOI markers (0xFF, 0xD8) and parse their extent
    let mut i = 0;
    while i + 3 < len {
        if raw_bytes[i] == 0xFF && raw_bytes[i + 1] == 0xD8 {
            if let Some(end) = parse_jpeg_end(i) {
                let stream_len = end - i;
                if stream_len > max_len {
                    max_len = stream_len;
                    largest = Some(raw_bytes[i..end].to_vec());
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }

    largest
}
