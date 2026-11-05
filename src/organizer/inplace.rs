//! # In-Place File Renaming & Atomic Rollback Safety
//!
//! Provides descriptive in-place file renaming without moving assets across directories:
//!
//! ```text
//! <stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location_Slug>].<ext>
//! ```
//!
//! ## Invariants
//! 1. **Zero Path Relocation**: Files remain in their exact parent folders; only filenames change.
//! 2. **Companion Atomicity**: Primary RAW, companion JPEGs, and `.xmp` sidecars are renamed simultaneously.
//! 3. **Idempotency Guard**: Prevents cyclic double-renaming on repeated executions.
//! 4. **Pre-Mutation Manifest Journaling**: Every rename operation records previous and new paths in SQLite
//!    before filesystem modification to allow complete, lossless rollback.
//! 5. **Singular Category Nomenclature**: Category tokens are strictly singular (`Bird`, `Mammalia`, `Insecta`, `Scenery`).

use crate::organizer::sanitize_species_slug;
use crate::scanner::decompose_filename;
use crate::types::{AssetBundle, AssetCategory, RoutingTier, SpeciesPrediction};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Constructs an in-place formatted filename:
/// `<stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location_Slug>].<ext>`
///
/// Date and location are cleanly omitted when absent or empty (no sentinels).
/// Empty categories or unclassified empty frames omit the category token.
///
/// # Arguments
/// * `stem` - Base un-renamed asset stem (e.g. `"DSC_0042"`).
/// * `category` - Singular taxonomic or genre category (e.g. `"Bird"`, `"Mammalia"`).
/// * `subject` - Subject descriptor or species common name (e.g. `"white-throated_kingfisher"`).
/// * `date` - Optional capture date in `YYYY-MM-DD` or `YYYYMMDD` format.
/// * `location` - Optional geographical location name or slug.
/// * `ext` - File extension (e.g. `"CR2"`, `".JPG"`, `"CR2.xmp"`).
///
/// # Returns
/// The fully formatted in-place filename string.
#[must_use]
pub fn format_inplace_filename(
    stem: &str,
    category: &str,
    subject: &str,
    date: Option<&str>,
    location: Option<&str>,
    ext: &str,
) -> String {
    let mut is_needs_review = false;

    let clean_cat = category.trim_matches('_');
    let cat_without_nr = if let Some(stripped) = clean_cat.strip_prefix("Needs_Review_") {
        is_needs_review = true;
        stripped
    } else if let Some(stripped) = clean_cat.strip_prefix("NeedsReview_") {
        is_needs_review = true;
        stripped
    } else if clean_cat == "Needs_Review" || clean_cat == "NeedsReview" {
        is_needs_review = true;
        ""
    } else {
        clean_cat
    };

    let clean_subj = subject.trim_matches('_');
    let subj_without_nr = if let Some(stripped) = clean_subj.strip_prefix("Needs_Review_") {
        is_needs_review = true;
        stripped
    } else if let Some(stripped) = clean_subj.strip_prefix("NeedsReview_") {
        is_needs_review = true;
        stripped
    } else {
        clean_subj
    };

    let is_unclassified = subj_without_nr.eq_ignore_ascii_case("no_subject_detected")
        || subj_without_nr.eq_ignore_ascii_case("no subject detected")
        || subj_without_nr.eq_ignore_ascii_case("nosubjectdetected")
        || subj_without_nr.eq_ignore_ascii_case("no_bird_detected")
        || subj_without_nr.eq_ignore_ascii_case("no bird detected")
        || subj_without_nr.eq_ignore_ascii_case("nobirddetected")
        || cat_without_nr.eq_ignore_ascii_case("no_subject_detected")
        || cat_without_nr.eq_ignore_ascii_case("no subject detected")
        || cat_without_nr.eq_ignore_ascii_case("no_bird_detected")
        || cat_without_nr.eq_ignore_ascii_case("no bird detected");

    let mut parts = Vec::new();

    if is_needs_review {
        parts.push("Needs_Review".to_string());
    }

    if is_unclassified {
        let unclass_token = if subj_without_nr.to_ascii_lowercase().contains("bird")
            || cat_without_nr.to_ascii_lowercase().contains("bird")
        {
            "No_Bird_Detected".to_string()
        } else {
            "No_Subject_Detected".to_string()
        };
        parts.push(unclass_token);
    } else {
        let cat_slug = sanitize_species_slug(cat_without_nr);
        if !cat_slug.is_empty() {
            parts.push(cat_slug);
        }
        let subj_slug = sanitize_species_slug(subj_without_nr);
        if !subj_slug.is_empty() {
            parts.push(subj_slug);
        }
    }

    if let Some(d) = date {
        let trimmed = d.trim();
        let lower = trimmed.to_ascii_lowercase();
        if !trimmed.is_empty() && lower != "unknown" && lower != "unknowndate" {
            let normalized = trimmed.replace(['-', ':', '/'], "");
            if !normalized.is_empty() {
                parts.push(normalized);
            }
        }
    }

    if let Some(l) = location {
        let trimmed = l.trim();
        let lower = trimmed.to_ascii_lowercase();
        if !trimmed.is_empty() && lower != "unknown" && lower != "unknownlocation" {
            let clean_loc = sanitize_species_slug(trimmed.trim_matches('_'));
            if !clean_loc.is_empty() {
                parts.push(clean_loc);
            }
        }
    }

    let full_stem = if parts.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}_{}", parts.join("_"))
    };

    if ext.is_empty() {
        full_stem
    } else if ext.starts_with('.') {
        format!("{full_stem}{ext}")
    } else {
        format!("{full_stem}.{ext}")
    }
}

/// Helper checking whether a string token represents an 8-digit date (`YYYYMMDD`)
/// or a 4-digit calendar year (`YYYY`).
fn is_date_token(tok: &str) -> bool {
    if tok.len() == 8 && tok.chars().all(|c| c.is_ascii_digit()) {
        if let (Ok(year), Ok(month), Ok(day)) = (
            tok[0..4].parse::<u32>(),
            tok[4..6].parse::<u32>(),
            tok[6..8].parse::<u32>(),
        ) {
            return (1900..=2100).contains(&year)
                && (1..=12).contains(&month)
                && (1..=31).contains(&day);
        }
    } else if tok.len() == 4 && tok.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(year) = tok.parse::<u32>() {
            return (1900..=2100).contains(&year);
        }
    }
    false
}

/// Matches a string slice against the Known Category Registry:
/// `Bird`, `Birds`, `Mammalia`, `Insecta`, `Scenery`, `Portrait`, `Abstract`.
fn match_known_category(token: &str) -> Option<AssetCategory> {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "bird" | "birds" => Some(AssetCategory::Bird),
        "mammalia" | "mammal" | "mammals" => Some(AssetCategory::Mammalia),
        "insecta" | "insect" | "insects" => Some(AssetCategory::Insecta),
        "scenery" => Some(AssetCategory::Scenery),
        "portrait" | "portraits" => Some(AssetCategory::Other("Portrait".to_string())),
        "abstract" => Some(AssetCategory::Other("Abstract".to_string())),
        _ => None,
    }
}

/// Parsed components of an in-place filename:
/// `(category, subject, routing_tier, optional_date, optional_location)`.
pub type InplaceFilenameParts = (
    AssetCategory,
    String,
    RoutingTier,
    Option<String>,
    Option<String>,
);

/// Extracts the asset category, subject name, routing tier, optional date,
/// and optional location from an in-place filename.
///
/// Handles 2-level naming (`<stem>_<Category>_<Subject>[_<Date>][_<Location>]`),
/// unclassified empty frames (`_No_Subject_Detected`, `_No_Bird_Detected`),
/// review flags (`Needs_Review_`), and legacy v1 single-level bird files
/// (e.g. `DSC_0042_Painted_Stork.CR2` promoted to [`AssetCategory::Bird`]).
///
/// # Arguments
/// * `filename` - Filename with or without extension.
/// * `original_stem` - Base un-renamed asset stem (e.g. `"DSC_0042"`).
///
/// # Returns
/// `Some((category, subject, tier, date, location))` if the filename is an in-place
/// renamed asset distinct from `original_stem`, or `None` otherwise.
#[must_use]
pub fn parse_inplace_filename(filename: &str, original_stem: &str) -> Option<InplaceFilenameParts> {
    let file_stem = if let Some((stem, _)) = decompose_filename(filename) {
        stem
    } else if let Some(dot_pos) = filename.find('.') {
        filename[..dot_pos].to_string()
    } else {
        filename.to_string()
    };

    if file_stem == original_stem {
        return None;
    }

    let suffix = if file_stem.len() > original_stem.len() && file_stem.starts_with(original_stem) {
        let rest = &file_stem[original_stem.len()..];
        rest.trim_start_matches('_')
    } else {
        file_stem.trim_start_matches('_')
    };

    if suffix.is_empty() {
        return None;
    }

    let (tier, after_tier) = if let Some(stripped) = suffix.strip_prefix("Needs_Review_") {
        (RoutingTier::NeedsReview, stripped)
    } else if let Some(stripped) = suffix.strip_prefix("NeedsReview_") {
        (RoutingTier::NeedsReview, stripped)
    } else if let Some(stripped) = suffix.strip_prefix("_Needs_Review_") {
        (RoutingTier::NeedsReview, stripped)
    } else {
        (RoutingTier::ConfirmedSpecies, suffix)
    };

    if after_tier == "No_Subject_Detected"
        || after_tier == "_No_Subject_Detected"
        || after_tier == "NoSubjectDetected"
        || after_tier.starts_with("No_Subject_Detected_")
        || after_tier.starts_with("_No_Subject_Detected_")
        || after_tier == "No_Bird_Detected"
        || after_tier == "_No_Bird_Detected"
        || after_tier == "NoBirdDetected"
        || after_tier.starts_with("No_Bird_Detected_")
        || after_tier.starts_with("_No_Bird_Detected_")
    {
        let is_bird = after_tier.contains("Bird");
        let subject_name = if is_bird {
            if after_tier.starts_with('_') {
                "_No_Bird_Detected".to_string()
            } else {
                "No_Bird_Detected".to_string()
            }
        } else if after_tier.starts_with('_') {
            "_No_Subject_Detected".to_string()
        } else {
            "No_Subject_Detected".to_string()
        };

        let remainder = if let Some(stripped) = after_tier.strip_prefix("_No_Subject_Detected_") {
            stripped
        } else if let Some(stripped) = after_tier.strip_prefix("No_Subject_Detected_") {
            stripped
        } else if let Some(stripped) = after_tier.strip_prefix("_No_Bird_Detected_") {
            stripped
        } else {
            after_tier
                .strip_prefix("No_Bird_Detected_")
                .unwrap_or_default()
        };

        let mut date = None;
        let mut location = None;

        if !remainder.is_empty() {
            let tokens: Vec<&str> = remainder.split('_').filter(|s| !s.is_empty()).collect();
            if let Some(d_idx) = tokens.iter().position(|t| is_date_token(t)) {
                date = Some(tokens[d_idx].to_string());
                if d_idx + 1 < tokens.len() {
                    location = Some(tokens[d_idx + 1..].join("_"));
                }
            } else {
                location = Some(tokens.join("_"));
            }
        }

        return Some((
            AssetCategory::Bird,
            subject_name,
            RoutingTier::NoBirdDetected,
            date,
            location,
        ));
    }

    let tokens: Vec<&str> = after_tier.split('_').filter(|s| !s.is_empty()).collect();
    if tokens.is_empty() {
        return None;
    }

    let mut date = None;
    let mut location = None;
    let entity_tokens: &[&str] = if let Some(d_idx) = tokens.iter().position(|t| is_date_token(t)) {
        date = Some(tokens[d_idx].to_string());
        if d_idx + 1 < tokens.len() {
            location = Some(tokens[d_idx + 1..].join("_"));
        }
        &tokens[..d_idx]
    } else {
        &tokens[..]
    };

    if entity_tokens.is_empty() {
        return Some((AssetCategory::Bird, String::new(), tier, date, location));
    }

    let first = entity_tokens[0];
    let matched_category = match_known_category(first);

    match matched_category {
        Some(category) if entity_tokens.len() > 1 => {
            let subject = entity_tokens[1..].join("_");
            Some((category, subject, tier, date, location))
        }
        Some(category) => Some((category, String::new(), tier, date, location)),
        None => {
            // Legacy v1 single-level nomenclature (e.g. DSC_0042_Painted_Stork)
            // Promoted to Bird category without splintering!
            let subject = entity_tokens.join("_");
            Some((AssetCategory::Bird, subject, tier, date, location))
        }
    }
}

/// Infers classification prediction, category, and routing tier from a descriptive in-place stem.
///
/// When assets have already been renamed (e.g. `1X7A9001_Mammalia_Malabar_Giant_Squirrel_20251002_Western_Ghats`
/// or `1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore`), this function extracts the
/// taxonomic domain, subject slug, and confidence tier so mock runs or offline pipelines don't
/// erroneously default to `_No_Bird_Detected` with `category: Bird`.
///
/// # Arguments
/// * `stem` - Filename stem (with or without extension or base original stem prefix).
///
/// # Returns
/// `Some((category, prediction, routing_tier))` if a valid subject is present in the stem,
/// or `None` if the stem contains no descriptive subject or is unclassified (`No_Subject_Detected`).
#[must_use]
pub fn infer_prediction_from_stem(
    stem: &str,
) -> Option<(AssetCategory, SpeciesPrediction, RoutingTier)> {
    let mut parts = stem.split('_');
    let first = parts.next()?;
    let rest_after_first = stem[first.len()..].strip_prefix('_')?;
    let second = rest_after_first.split('_').next()?;

    let (base_stem, suffix) = if second.chars().all(|c| c.is_ascii_digit()) {
        let base_len = first.len() + 1 + second.len();
        let s = stem[base_len..].strip_prefix('_').unwrap_or("");
        (&stem[..base_len], s)
    } else {
        (first, rest_after_first)
    };

    if suffix.is_empty() {
        return None;
    }

    let (category, subject, tier, _date, _loc) = parse_inplace_filename(stem, base_stem)?;

    let clean_subj = subject.trim_matches('_');
    if clean_subj.is_empty()
        || clean_subj.eq_ignore_ascii_case("no_subject_detected")
        || clean_subj.eq_ignore_ascii_case("no_bird_detected")
        || !clean_subj.chars().any(|c| c.is_ascii_alphabetic())
    {
        return None;
    }

    let folder_slug = if category == AssetCategory::Bird {
        sanitize_species_slug(clean_subj)
    } else {
        let sanitized = sanitize_species_slug(clean_subj);
        if sanitized.starts_with(category.as_str()) {
            sanitized
        } else {
            format!("{}_{}", category.as_str(), sanitized)
        }
    };

    let common_name = clean_subj.replace('_', " ");
    let combined_score = match tier {
        RoutingTier::ConfirmedSpecies => 0.95,
        RoutingTier::NeedsReview => 0.60,
        RoutingTier::NoBirdDetected => 0.10,
    };

    let pred = SpeciesPrediction {
        taxon_id: folder_slug.to_lowercase(),
        common_name,
        folder_slug,
        scientific_name: String::new(),
        visual_prob: combined_score,
        geo_prior: 1.0,
        combined_score,
    };

    Some((category, pred, tier))
}

/// Idempotency guard: checks if the stem already contains the species slug
/// to prevent double-suffixing on repeated runs.
pub fn is_already_inplace_named(stem: &str, species: &str) -> bool {
    let clean_species = species.trim_matches('_');
    let species_slug = sanitize_species_slug(clean_species);
    if species_slug.is_empty() {
        return false;
    }

    stem.contains(&format!("_{species_slug}"))
        || stem.ends_with(&species_slug)
        || stem == species_slug
}

/// Renames the primary image, companion images, and sidecar files in lockstep
/// in their original location on disk.
/// Skips renaming if `is_already_inplace_named` detects the stem is already suffixed.
/// Resolves disk collisions by appending `_1`, `_2` before extensions.
/// Returns the list of `(original_path, new_path)` pairs for manifest logging.
pub fn execute_inplace_bundle_rename(
    bundle: &AssetBundle,
    species: &str,
    date: Option<&str>,
    location: Option<&str>,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    execute_inplace_bundle_rename_with_category(bundle, "", species, date, location)
}

/// Renames all files in an asset bundle in-place using category and subject parameters.
/// Resolves disk collisions by appending `_1`, `_2` before extensions.
/// Returns the list of `(original_path, new_path)` pairs for manifest logging.
pub fn execute_inplace_bundle_rename_with_category(
    bundle: &AssetBundle,
    category: &str,
    subject: &str,
    date: Option<&str>,
    location: Option<&str>,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    if is_already_inplace_named(&bundle.original_stem, subject) {
        return Ok(Vec::new());
    }

    let derived_date = date.map(std::string::ToString::to_string).or_else(|| {
        bundle.capture_timestamp_ms.and_then(|ms| {
            chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.format("%Y%m%d").to_string())
        })
    });
    let effective_date = derived_date.as_deref();
    let effective_loc = location.or(bundle.resolved_location_name.as_deref());

    let new_base_stem = format_inplace_filename(
        &bundle.original_stem,
        category,
        subject,
        effective_date,
        effective_loc,
        "",
    );

    execute_inplace_bundle_rename_with_stem(bundle, &new_base_stem)
}

/// Renames all files in an asset bundle using an explicit base stem.
pub fn execute_inplace_bundle_rename_with_stem(
    bundle: &AssetBundle,
    new_stem: &str,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    if bundle.original_stem == new_stem {
        return Ok(Vec::new());
    }

    let mut source_files = Vec::new();
    let mut seen_canonical = HashSet::new();

    let mut add_file = |path: &Path| {
        if path.exists() {
            if let Ok(canon) = path.canonicalize() {
                if seen_canonical.insert(canon) {
                    source_files.push(path.to_path_buf());
                }
            } else if seen_canonical.insert(path.to_path_buf()) {
                source_files.push(path.to_path_buf());
            }
        }
    };

    add_file(&bundle.primary_image);
    for comp in &bundle.companion_images {
        add_file(comp);
    }
    for sidecar in &bundle.sidecar_files {
        add_file(sidecar);
    }

    // Discover any additional sidecar files sharing the same original stem on disk
    if let Some(parent) = bundle.primary_image.parent() {
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                        if let Some((stem, _)) = decompose_filename(file_name) {
                            if stem == bundle.original_stem {
                                add_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    if source_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut renames_plan = Vec::new();
    for src_path in &source_files {
        let file_name = src_path
            .file_name()
            .and_then(|f| f.to_str())
            .context("File has invalid UTF-8 filename")?;

        let remainder = if let Some((base_stem, _)) = decompose_filename(file_name) {
            if file_name.len() >= base_stem.len() {
                file_name[base_stem.len()..].to_string()
            } else {
                String::new()
            }
        } else if let Some(pos) = file_name.find('.') {
            file_name[pos..].to_string()
        } else {
            String::new()
        };

        renames_plan.push((src_path.clone(), remainder));
    }

    // Search for disambiguated base stem if target already exists on disk
    let mut counter = 0;
    let final_base_stem = loop {
        let candidate_stem = if counter == 0 {
            new_stem.to_string()
        } else {
            format!("{new_stem}_{counter}")
        };

        let has_collision = renames_plan.iter().any(|(src, rem)| {
            if let Some(parent) = src.parent() {
                let target = parent.join(format!("{candidate_stem}{rem}"));
                if target.exists() {
                    if let Ok(canon_target) = target.canonicalize() {
                        !seen_canonical.contains(&canon_target)
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            }
        });

        if !has_collision {
            break candidate_stem;
        }
        counter += 1;
    };

    let mut completed_renames = Vec::new();
    for (src, rem) in &renames_plan {
        let parent = src.parent().unwrap_or_else(|| Path::new("."));
        let dst = parent.join(format!("{final_base_stem}{rem}"));

        if *src == dst {
            continue;
        }

        if let Err(e) = fs::rename(src, &dst) {
            // Roll back renames that succeeded earlier in this batch
            for (original_src, current_dst) in completed_renames.iter().rev() {
                let _ = fs::rename(current_dst, original_src);
            }
            return Err(e).with_context(|| {
                format!("Failed to rename {} to {}", src.display(), dst.display())
            });
        }

        completed_renames.push((src.clone(), dst));
    }

    Ok(completed_renames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_inplace_filename_full() {
        let name = format_inplace_filename(
            "IMG_1001",
            "Bird",
            "House Sparrow",
            Some("2026-10-25"),
            Some("Leh"),
            "CR2",
        );
        assert_eq!(name, "IMG_1001_Bird_House_Sparrow_20261025_Leh.CR2");
    }

    #[test]
    fn test_format_inplace_filename_no_date_no_location() {
        let name = format_inplace_filename("IMG_1001", "Bird", "House Sparrow", None, None, "JPG");
        assert_eq!(name, "IMG_1001_Bird_House_Sparrow.JPG");
    }

    #[test]
    fn test_format_inplace_filename_ignores_sentinel_strings() {
        let name = format_inplace_filename(
            "IMG_1001",
            "Bird",
            "House Sparrow",
            Some("unknown"),
            Some("UnknownLocation"),
            "CR2",
        );
        assert_eq!(name, "IMG_1001_Bird_House_Sparrow.CR2");
    }

    #[test]
    fn test_format_inplace_filename_no_bird_detected() {
        let name = format_inplace_filename(
            "IMG_1002",
            "",
            "_No_Bird_Detected",
            Some("2026-10-25"),
            None,
            "JPG",
        );
        assert_eq!(name, "IMG_1002_No_Bird_Detected_20261025.JPG");
    }
}
