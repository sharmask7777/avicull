//! # Gazetteer Spatial Index & Trigram Fuzzy Name Matching
//!
//! Provides an embedded gazetteer of global avian biodiversity hotspots, Important Bird Areas (IBAs),
//! national parks, wildlife sanctuaries, and wetlands.
//!
//! Hotspot names extracted from directory tokens (e.g. `"Ranthambhore"`, `"Keoladeo"`, `"Corbett"`)
//! are resolved using a character-level trigram similarity index, achieving robust tolerance to
//! alternate transliterations, casing, and minor typographical discrepancies.

use std::collections::HashMap;

/// An authoritative geographic location entry from the gazetteer.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(from = "GazetteerEntryRaw")]
pub struct GazetteerEntry {
    pub name: String,
    pub region_code: String,
    pub latitude: f64,
    pub longitude: f64,
    pub category: String,
}

#[derive(serde::Deserialize)]
struct GazetteerEntryRaw {
    name: String,
    region_code: String,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    category: String,
}

impl From<GazetteerEntryRaw> for GazetteerEntry {
    fn from(raw: GazetteerEntryRaw) -> Self {
        Self {
            name: raw.name,
            region_code: raw.region_code,
            latitude: raw.latitude.or(raw.lat).unwrap_or(0.0),
            longitude: raw.longitude.or(raw.lon).unwrap_or(0.0),
            category: raw.category,
        }
    }
}

#[derive(Debug, Clone)]
struct IndexedEntry {
    entry: GazetteerEntry,
    normalized_name: String,
    trigrams: Vec<[char; 3]>,
}

/// In-memory search engine for the global gazetteer using trigram fuzzy matching.
#[derive(Debug, Clone, Default)]
pub struct GazetteerEngine {
    entries: Vec<IndexedEntry>,
}

static EMBEDDED_GAZETTEER_JSON: &str = include_str!("../../data/gazetteer.json");

impl GazetteerEngine {
    /// Loads the embedded `data/gazetteer.json` catalog.
    pub fn load_embedded() -> anyhow::Result<Self> {
        let entries: Vec<GazetteerEntry> = serde_json::from_str(EMBEDDED_GAZETTEER_JSON)
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded gazetteer.json: {}", e))?;
        Ok(Self::from_entries(entries))
    }

    /// Creates a new `GazetteerEngine` from a vector of entries.
    pub fn from_entries(entries: Vec<GazetteerEntry>) -> Self {
        let indexed = entries
            .into_iter()
            .map(|entry| {
                let normalized_name = normalize_string(&entry.name);
                let trigrams = extract_trigrams(&normalized_name);
                IndexedEntry {
                    entry,
                    normalized_name,
                    trigrams,
                }
            })
            .collect();
        Self { entries: indexed }
    }

    /// Returns the number of entries in the gazetteer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the gazetteer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all gazetteer entries.
    pub fn entries(&self) -> impl Iterator<Item = &GazetteerEntry> {
        self.entries.iter().map(|e| &e.entry)
    }

    /// Searches the gazetteer using normalized substring & trigram fuzzy similarity.
    /// Returns up to `limit` candidates ranked by match score in descending order.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(GazetteerEntry, f64)> {
        if limit == 0 {
            return Vec::new();
        }

        let q_norm = normalize_string(query);
        if q_norm.is_empty() {
            return Vec::new();
        }

        let q_trigrams = extract_trigrams(&q_norm);
        let mut scored: Vec<(GazetteerEntry, f64)> = Vec::new();

        for item in &self.entries {
            let is_exact = q_norm == item.normalized_name;
            let is_prefix = item.normalized_name.starts_with(&q_norm)
                || q_norm.starts_with(&item.normalized_name);
            let is_sub = is_prefix
                || item.normalized_name.contains(&q_norm)
                || q_norm.contains(&item.normalized_name);

            let tri_sim = calculate_trigram_similarity(&q_trigrams, &item.trigrams);

            // Skip entries with negligible trigram similarity and no substring overlap
            if tri_sim < 0.05 && !is_sub && !is_exact {
                continue;
            }

            let category_boost = match item.entry.category.to_lowercase().as_str() {
                "hotspot" => 0.05,
                "reserve" => 0.03,
                "city" => 0.01,
                _ => 0.0,
            };

            let base_score = if is_exact {
                0.95
            } else if is_prefix {
                (tri_sim + 0.30).min(0.90)
            } else if is_sub {
                (tri_sim + 0.20).min(0.85)
            } else {
                tri_sim
            };

            let final_score = (base_score + category_boost).min(1.0);
            if final_score > 0.05 {
                scored.push((item.entry.clone(), final_score));
            }
        }

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.name.cmp(&b.0.name))
        });

        scored.truncate(limit);
        scored
    }
}

/// Normalizes a string by lowercasing, folding accented Latin characters,
/// replacing punctuation and symbols with spaces, and collapsing whitespace.
pub fn normalize_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for lc in c.to_lowercase() {
            match lc {
                'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => out.push('a'),
                'é' | 'è' | 'ê' | 'ë' | 'ē' => out.push('e'),
                'í' | 'ì' | 'î' | 'ï' | 'ī' => out.push('i'),
                'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ō' => out.push('o'),
                'ú' | 'ù' | 'û' | 'ü' | 'ū' => out.push('u'),
                'ñ' => out.push('n'),
                'ç' => out.push('c'),
                other => {
                    if other.is_alphanumeric() {
                        out.push(other);
                    } else {
                        out.push(' ');
                    }
                }
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extracts character trigrams from a string padded with boundary spaces.
pub fn extract_trigrams(s: &str) -> Vec<[char; 3]> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = std::iter::once(' ')
        .chain(trimmed.chars())
        .chain(std::iter::once(' '))
        .collect();
    if chars.len() < 3 {
        return Vec::new();
    }
    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

/// Calculates the Sørensen-Dice coefficient between two trigram bags:
/// S(A, B) = 2 * |tri(A) ∩ tri(B)| / (|tri(A)| + |tri(B)|)
pub fn calculate_trigram_similarity(tri_a: &[[char; 3]], tri_b: &[[char; 3]]) -> f64 {
    let total = tri_a.len() + tri_b.len();
    if total == 0 {
        return 0.0;
    }

    let mut counts_a: HashMap<[char; 3], usize> = HashMap::with_capacity(tri_a.len());
    for &t in tri_a {
        *counts_a.entry(t).or_insert(0) += 1;
    }

    let mut counts_b: HashMap<[char; 3], usize> = HashMap::with_capacity(tri_b.len());
    for &t in tri_b {
        *counts_b.entry(t).or_insert(0) += 1;
    }

    let mut intersection = 0;
    for (t, count_a) in counts_a {
        if let Some(&count_b) = counts_b.get(&t) {
            intersection += count_a.min(count_b);
        }
    }

    (2.0 * intersection as f64) / (total as f64)
}

/// Calculates trigram similarity between two arbitrary strings.
pub fn trigram_similarity(a: &str, b: &str) -> f64 {
    let norm_a = normalize_string(a);
    let norm_b = normalize_string(b);
    if norm_a.is_empty() && norm_b.is_empty() {
        return 1.0;
    }
    if norm_a.is_empty() || norm_b.is_empty() {
        return 0.0;
    }
    let tri_a = extract_trigrams(&norm_a);
    let tri_b = extract_trigrams(&norm_b);
    calculate_trigram_similarity(&tri_a, &tri_b)
}
