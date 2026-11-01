//! # Directory Path Tokenization & Noise Word Filtering
//!
//! Photographers organize storage trees using idiosyncratic directory naming:
//! - Dates: `2024-05-18`, `May_2023`, `20241012`
//! - Camera hardware folders: `100CANON`, `101MSDCF`, `DCIM`, `EOS_R5`
//! - Workflow stages: `RAW`, `Selects`, `Edited`, `Exported`, `Backups`
//! - Structural filesystem prefixes: `Users`, `Pictures`, `Volumes`
//!
//! This module analyzes arbitrary filesystem paths, strips irrelevant structural and camera noise,
//! and extracts high-confidence geographic location name candidates for gazetteer matching.

use std::collections::HashSet;
use std::path::{Component, Path};

/// Checks if a token is structural or date noise that should be rejected
/// as a geographic location candidate.
pub fn is_structural_noise_token(token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();

    // Check if token consists purely of punctuation, digits, or whitespace
    if lower
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation() || c.is_whitespace())
    {
        return true;
    }

    if is_known_noise_word(&lower) {
        return true;
    }

    if is_camera_folder(&lower) {
        return true;
    }

    if is_date_token(&lower) {
        return true;
    }

    false
}

const KNOWN_NOISE_WORDS: &[&str] = &[
    "raw",
    "raws",
    "processed",
    "selects",
    "select",
    "selected",
    "selection",
    "export",
    "exports",
    "exported",
    "final",
    "finals",
    "edit",
    "edits",
    "edited",
    "master",
    "masters",
    "original",
    "originals",
    "backup",
    "backups",
    "dcim",
    "camera",
    "photos",
    "photo",
    "pictures",
    "picture",
    "pics",
    "pic",
    "images",
    "image",
    "img",
    "imgs",
    "users",
    "home",
    "root",
    "desktop",
    "documents",
    "downloads",
    "volumes",
    "volume",
    "media",
    "mnt",
    "disk",
    "drive",
    "storage",
    "birds_hierarchy",
    "avicull",
    ".avicull",
    "all",
    "misc",
    "temp",
    "tmp",
    "jpg",
    "jpeg",
    "cr2",
    "cr3",
    "nef",
    "arw",
    "dng",
    "raf",
    "rw2",
    "tif",
    "tiff",
    "png",
    "card",
    "sdcard",
    "eos_digital",
    "untitled",
    "macintosh hd",
];

const NOISE_WORDS_TO_STRIP: &[&str] = &[
    "pictures",
    "picture",
    "photos",
    "photo",
    "images",
    "image",
    "img",
    "imgs",
    "pics",
    "pic",
    "raw",
    "raws",
    "processed",
    "selects",
    "select",
    "export",
    "exports",
    "dcim",
    "canon",
    "nikon",
    "sony",
    "fuji",
    "fujifilm",
];

fn is_known_noise_word(s: &str) -> bool {
    let s = s.trim_matches(['_', '-', '.']);
    KNOWN_NOISE_WORDS.contains(&s)
}

fn is_camera_folder(s: &str) -> bool {
    let s = s.trim();
    if s.len() >= 4 {
        let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 3 {
            let rest = s[digits..].trim_start_matches(['_', '-']);
            if !rest.is_empty() {
                return true;
            }
        }
    }
    const CAMERA_BRANDS: &[&str] = &[
        "canon",
        "nikon",
        "sony",
        "fujifilm",
        "fuji",
        "panasonic",
        "olympus",
        "leica",
        "lumix",
        "hasselblad",
        "pentax",
    ];
    CAMERA_BRANDS.contains(&s)
}

fn is_month(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "jan"
            | "january"
            | "feb"
            | "february"
            | "mar"
            | "march"
            | "apr"
            | "april"
            | "may"
            | "jun"
            | "june"
            | "jul"
            | "july"
            | "aug"
            | "august"
            | "sep"
            | "sept"
            | "september"
            | "oct"
            | "october"
            | "nov"
            | "november"
            | "dec"
            | "december"
    )
}

fn is_year(s: &str) -> bool {
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(y) = s.parse::<u32>() {
            return (1900..=2099).contains(&y);
        }
    }
    false
}

fn is_day(s: &str) -> bool {
    let clean = s.trim_end_matches(['t', 'h', 's', 'r', 'd', 'n']);
    if !clean.is_empty() && clean.len() <= 2 && clean.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(d) = clean.parse::<u32>() {
            return (1..=31).contains(&d);
        }
    }
    false
}

fn is_month_num(s: &str) -> bool {
    if !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(m) = s.parse::<u32>() {
            return (1..=12).contains(&m);
        }
    }
    false
}

fn is_date_token(s: &str) -> bool {
    let s = s.trim();

    // Check 8-digit date like 20250419
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        if let (Ok(y), Ok(m), Ok(d)) = (
            s[0..4].parse::<u32>(),
            s[4..6].parse::<u32>(),
            s[6..8].parse::<u32>(),
        ) {
            if (1900..=2099).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
                return true;
            }
        }
    }

    let parts: Vec<&str> = s
        .split([' ', '-', '_', '/', '.', ','])
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if parts.is_empty() {
        return false;
    }

    let mut has_year = false;
    let mut has_month = false;
    let mut all_date_components = true;

    for part in &parts {
        if is_year(part) {
            has_year = true;
        } else if is_month(part) {
            has_month = true;
        } else if is_day(part) || is_month_num(part) {
            // Valid day or month number
        } else {
            all_date_components = false;
            break;
        }
    }

    if all_date_components && (has_year || has_month) {
        return true;
    }

    false
}

fn is_file_component(s: &str) -> bool {
    const MEDIA_EXTS: &[&str] = &[
        "cr2", "cr3", "nef", "arw", "dng", "raf", "rw2", "pef", "orf", "srw", "jpg", "jpeg", "png",
        "tif", "tiff", "webp", "heic", "heif", "xmp", "mov", "mp4",
    ];
    if let Some(pos) = s.rfind('.') {
        let ext = &s[pos + 1..].to_lowercase();
        return MEDIA_EXTS.contains(&ext.as_str());
    }
    false
}

#[derive(Debug, Clone)]
struct AlphanumSpan {
    start: usize,
    end: usize,
    text: String,
}

fn extract_alphanum_spans(s: &str) -> Vec<AlphanumSpan> {
    let mut spans = Vec::new();
    let mut current_start = None;

    for (byte_idx, c) in s.char_indices() {
        if c.is_alphanumeric() {
            if current_start.is_none() {
                current_start = Some(byte_idx);
            }
        } else if let Some(start) = current_start {
            spans.push(AlphanumSpan {
                start,
                end: byte_idx,
                text: s[start..byte_idx].to_string(),
            });
            current_start = None;
        }
    }

    if let Some(start) = current_start {
        spans.push(AlphanumSpan {
            start,
            end: s.len(),
            text: s[start..].to_string(),
        });
    }

    spans
}

fn strip_dates_from_string(s: &str) -> String {
    let spans = extract_alphanum_spans(s);
    if spans.is_empty() {
        return s.to_string();
    }

    let mut remove_ranges: Vec<(usize, usize)> = Vec::new();

    // Check windows of 3: (month, day, year), (day, month, year), (year, month_num, day)
    let mut i = 0;
    while i < spans.len() {
        if i + 2 < spans.len() {
            let t0 = &spans[i].text;
            let t1 = &spans[i + 1].text;
            let t2 = &spans[i + 2].text;

            let is_m_d_y = is_month(t0) && is_day(t1) && is_year(t2);
            let is_d_m_y = is_day(t0) && is_month(t1) && is_year(t2);
            let is_y_m_d = is_year(t0) && is_month_num(t1) && is_day(t2);
            let is_y_mname_d = is_year(t0) && is_month(t1) && is_day(t2);

            if is_m_d_y || is_d_m_y || is_y_m_d || is_y_mname_d {
                remove_ranges.push((spans[i].start, spans[i + 2].end));
                i += 3;
                continue;
            }
        }

        // Check windows of 2: (month, year), (year, month), (month, day), (day, month), (year, month_num), (month_num, year)
        if i + 1 < spans.len() {
            let t0 = &spans[i].text;
            let t1 = &spans[i + 1].text;

            let is_m_y = (is_month(t0) || is_month_num(t0)) && is_year(t1);
            let is_y_m = is_year(t0) && (is_month(t1) || is_month_num(t1));
            let is_m_d = is_month(t0) && is_day(t1);
            let is_d_m = is_day(t0) && is_month(t1);

            if is_m_y || is_y_m || is_m_d || is_d_m {
                remove_ranges.push((spans[i].start, spans[i + 1].end));
                i += 2;
                continue;
            }
        }

        // Check single span: year or 8-digit date
        let t0 = &spans[i].text;
        if is_year(t0) {
            remove_ranges.push((spans[i].start, spans[i].end));
            i += 1;
            continue;
        }

        if t0.len() == 8 && t0.chars().all(|c| c.is_ascii_digit()) {
            if let (Ok(y), Ok(m), Ok(d)) = (
                t0[0..4].parse::<u32>(),
                t0[4..6].parse::<u32>(),
                t0[6..8].parse::<u32>(),
            ) {
                if (1900..=2099).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
                    remove_ranges.push((spans[i].start, spans[i].end));
                    i += 1;
                    continue;
                }
            }
        }

        i += 1;
    }

    if remove_ranges.is_empty() {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_byte = 0;

    for (start, end) in remove_ranges {
        if start > current_byte {
            result.push_str(&s[current_byte..start]);
        }
        result.push(' ');
        current_byte = end;
    }
    if current_byte < s.len() {
        result.push_str(&s[current_byte..]);
    }

    result
}

fn clean_phrase(phrase: &str) -> Option<String> {
    let date_stripped = strip_dates_from_string(phrase);

    // Replace punctuation and separators with spaces
    let mut space_separated = String::with_capacity(date_stripped.len());
    for c in date_stripped.chars() {
        if matches!(c, '_' | '-' | '/' | '.' | '+') {
            space_separated.push(' ');
        } else {
            space_separated.push(c);
        }
    }

    let words: Vec<&str> = space_separated
        .split_whitespace()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return None;
    }

    // Strip leading noise words
    let mut start_idx = 0;
    while start_idx < words.len() {
        let lower = words[start_idx].to_lowercase();
        if NOISE_WORDS_TO_STRIP.contains(&lower.as_str()) || is_known_noise_word(&lower) {
            start_idx += 1;
        } else {
            break;
        }
    }

    // Strip trailing noise words
    let mut end_idx = words.len();
    while end_idx > start_idx {
        let lower = words[end_idx - 1].to_lowercase();
        if NOISE_WORDS_TO_STRIP.contains(&lower.as_str()) || is_known_noise_word(&lower) {
            end_idx -= 1;
        } else {
            break;
        }
    }

    if start_idx >= end_idx {
        return None;
    }

    let cleaned = words[start_idx..end_idx].join(" ").to_lowercase();
    if cleaned.is_empty() || is_structural_noise_token(&cleaned) {
        None
    } else {
        Some(cleaned)
    }
}

pub fn extract_candidate_location_tokens(path: &Path) -> Vec<String> {
    let mut raw_components: Vec<String> = Vec::new();
    for comp in path.components() {
        if let Component::Normal(os_str) = comp {
            if let Some(s) = os_str.to_str() {
                raw_components.push(s.to_string());
            }
        }
    }

    if raw_components.is_empty() {
        return Vec::new();
    }

    // If the last component has an image/media extension, drop it
    if let Some(last) = raw_components.last() {
        if is_file_component(last) || path.extension().is_some() {
            raw_components.pop();
        }
    }

    // Identify system prefixes to skip:
    // e.g. /Users/<username>/... or /home/<username>/... or /Volumes/<volume_name>/...
    let mut skip_prefix_count = 0;
    if !raw_components.is_empty() {
        let first = raw_components[0].to_lowercase();
        if (first == "users" || first == "home") && raw_components.len() >= 2 {
            skip_prefix_count = 2; // skip Users and <username>
        } else if first == "volumes" && raw_components.len() >= 2 {
            skip_prefix_count = 2; // skip Volumes and <volume_name>
        }
    }

    let mut candidates: Vec<String> = Vec::new();

    // Iterate leaf-to-root
    for (idx, comp) in raw_components.iter().enumerate().rev() {
        if idx < skip_prefix_count {
            continue;
        }

        if is_structural_noise_token(comp) {
            continue;
        }

        // Split component by major delimiters (comma, semicolon, pipe)
        for part in comp.split([',', ';', '|']) {
            if let Some(phrase) = clean_phrase(part) {
                candidates.push(phrase);
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for token in candidates {
        if seen.insert(token.clone()) {
            result.push(token);
        }
    }

    result
}
