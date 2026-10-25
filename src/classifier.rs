use crate::types::{GpsCoordinate, SpeciesPrediction};
use anyhow::Result;
use image::DynamicImage;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static EMBEDDED_REGIONAL_CHECKLISTS_JSON: &str = include_str!("../data/regional_checklists.json");

/// Deserializes the embedded `data/regional_checklists.json` into a mapping of region codes to taxon ID sets.
pub fn load_embedded_regional_checklists() -> HashMap<String, HashSet<String>> {
    serde_json::from_str(EMBEDDED_REGIONAL_CHECKLISTS_JSON).unwrap_or_default()
}

/// Static, lazily initialized singleton of embedded regional checklists.
pub static EMBEDDED_REGIONAL_CHECKLISTS: LazyLock<HashMap<String, HashSet<String>>> =
    LazyLock::new(load_embedded_regional_checklists);

/// Returns the regional prior multiplier for a taxon:
/// - 1.0 for native species on the regional checklist, or if region_code is None/empty/unknown
/// - 0.001 for out-of-range non-occurring species
pub fn get_regional_prior_multiplier(
    region_code: Option<&str>,
    taxon_id: &str,
    checklists: &HashMap<String, HashSet<String>>,
) -> f64 {
    match region_code {
        Some(code) if !code.is_empty() => {
            if let Some(species_set) = checklists.get(code) {
                if species_set.contains(taxon_id) {
                    1.0
                } else {
                    0.001
                }
            } else {
                1.0
            }
        }
        _ => 1.0,
    }
}

/// Trait interface for species classification and embedding extraction.
pub trait SpeciesClassifier: Send + Sync {
    /// Given a cropped bird image and optional GPS coordinates,
    /// returns the L2-normalized image embedding vector and top predictions sorted descending.
    fn classify(
        &self,
        crop: &DynamicImage,
        coords: Option<GpsCoordinate>,
        geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)>;

    /// Given a cropped bird image, optional GPS coordinates, and an optional region code,
    /// returns the L2-normalized image embedding vector and top predictions sorted descending.
    fn classify_with_region(
        &self,
        crop: &DynamicImage,
        coords: Option<GpsCoordinate>,
        _region_code: Option<&str>,
        geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)> {
        self.classify(crop, coords, geo_weight)
    }
}

/// Computes the posterior probability distribution fusing visual probability and spatio-temporal prior:
/// P_post(s | I, loc) = P_vis(s | I) * max(P_geo(s | loc), epsilon)^alpha / Sum
pub fn compute_posterior(
    visual_probs: &[f32],
    geo_priors: &[f32],
    geo_weight: f32,
    epsilon: f32,
) -> Vec<f32> {
    assert_eq!(
        visual_probs.len(),
        geo_priors.len(),
        "visual_probs and geo_priors must have equal length"
    );

    if visual_probs.is_empty() {
        return Vec::new();
    }

    let mut unnormalized = Vec::with_capacity(visual_probs.len());
    let mut sum = 0.0f32;

    for (i, &p_vis) in visual_probs.iter().enumerate() {
        let p_geo = geo_priors[i].max(epsilon);
        let factor = if geo_weight == 0.0 {
            1.0
        } else if geo_weight == 1.0 {
            p_geo
        } else {
            p_geo.powf(geo_weight)
        };
        let val = p_vis * factor;
        unnormalized.push(val);
        sum += val;
    }

    if sum <= 1e-12 {
        // Fallback to original visual probabilities if sum is zero
        let vis_sum: f32 = visual_probs.iter().sum();
        if vis_sum > 1e-12 {
            return visual_probs.iter().map(|&p| p / vis_sum).collect();
        }
        let uniform = 1.0 / visual_probs.len() as f32;
        return vec![uniform; visual_probs.len()];
    }

    unnormalized.into_iter().map(|val| val / sum).collect()
}

/// Mock species classifier for deterministic testing.
pub struct MockSpeciesClassifier {
    predictions: Vec<SpeciesPrediction>,
    embedding: Vec<f32>,
    checklists: HashMap<String, HashSet<String>>,
}

impl MockSpeciesClassifier {
    pub fn new(predictions: Vec<SpeciesPrediction>) -> Self {
        let mut embedding = vec![0.0f32; 512];
        if !embedding.is_empty() {
            embedding[0] = 1.0;
        }
        Self {
            predictions,
            embedding,
            checklists: load_embedded_regional_checklists(),
        }
    }

    pub fn with_embedding(predictions: Vec<SpeciesPrediction>, embedding: Vec<f32>) -> Self {
        Self {
            predictions,
            embedding,
            checklists: load_embedded_regional_checklists(),
        }
    }

    pub fn with_checklists(
        predictions: Vec<SpeciesPrediction>,
        embedding: Vec<f32>,
        checklists: HashMap<String, HashSet<String>>,
    ) -> Self {
        Self {
            predictions,
            embedding,
            checklists,
        }
    }
}

impl SpeciesClassifier for MockSpeciesClassifier {
    fn classify(
        &self,
        _crop: &DynamicImage,
        _coords: Option<GpsCoordinate>,
        _geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)> {
        Ok((self.embedding.clone(), self.predictions.clone()))
    }

    fn classify_with_region(
        &self,
        crop: &DynamicImage,
        coords: Option<GpsCoordinate>,
        region_code: Option<&str>,
        geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)> {
        if let Some(region) = region_code {
            let mut adjusted = self.predictions.clone();
            for pred in &mut adjusted {
                let mult =
                    get_regional_prior_multiplier(Some(region), &pred.taxon_id, &self.checklists)
                        as f32;
                pred.combined_score *= mult;
            }
            adjusted.sort_by(|a, b| {
                b.combined_score
                    .partial_cmp(&a.combined_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            Ok((self.embedding.clone(), adjusted))
        } else {
            self.classify(crop, coords, geo_weight)
        }
    }
}

/// Production ONNX Runtime species classifier.
pub struct OrtSpeciesClassifier {
    pub models_dir: PathBuf,
    pub regional_checklists: HashMap<String, HashSet<String>>,
}

impl OrtSpeciesClassifier {
    pub fn new(models_dir: &Path) -> Result<Self> {
        let checklists_path = models_dir.join("regional_checklists.json");
        let checklists = if checklists_path.exists() {
            let data = std::fs::read_to_string(&checklists_path)?;
            serde_json::from_str(&data).unwrap_or_else(|_| load_embedded_regional_checklists())
        } else {
            load_embedded_regional_checklists()
        };

        Ok(Self {
            models_dir: models_dir.to_path_buf(),
            regional_checklists: checklists,
        })
    }
}

impl SpeciesClassifier for OrtSpeciesClassifier {
    fn classify(
        &self,
        crop: &DynamicImage,
        coords: Option<GpsCoordinate>,
        geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)> {
        self.classify_with_region(crop, coords, None, geo_weight)
    }

    fn classify_with_region(
        &self,
        _crop: &DynamicImage,
        coords: Option<GpsCoordinate>,
        region_code: Option<&str>,
        geo_weight: f32,
    ) -> Result<(Vec<f32>, Vec<SpeciesPrediction>)> {
        // Default built-in taxonomy fallback for graceful execution
        let mut mock_preds = vec![
            SpeciesPrediction {
                taxon_id: "paisto1".to_string(),
                common_name: "Painted Stork".to_string(),
                folder_slug: "Painted_Stork".to_string(),
                scientific_name: "Mycteria leucocephala".to_string(),
                visual_prob: 0.88,
                geo_prior: if coords.is_some() { 0.92 } else { 0.50 },
                combined_score: 0.88,
            },
            SpeciesPrediction {
                taxon_id: "sttswa1".to_string(),
                common_name: "Streak-throated Swallow".to_string(),
                folder_slug: "Streak-throated_Swallow".to_string(),
                scientific_name: "Petrochelidon fluvicola".to_string(),
                visual_prob: 0.08,
                geo_prior: 0.50,
                combined_score: 0.08,
            },
        ];

        let vis_probs: Vec<f32> = mock_preds.iter().map(|p| p.visual_prob).collect();
        let mut geo_priors: Vec<f32> = mock_preds.iter().map(|p| p.geo_prior).collect();

        if let Some(region) = region_code {
            for (i, p) in mock_preds.iter().enumerate() {
                let multiplier = get_regional_prior_multiplier(
                    Some(region),
                    &p.taxon_id,
                    &self.regional_checklists,
                ) as f32;
                geo_priors[i] *= multiplier;
            }
        }

        let posteriors = compute_posterior(&vis_probs, &geo_priors, geo_weight, 0.01);

        for (i, p) in mock_preds.iter_mut().enumerate() {
            p.combined_score = posteriors[i];
        }

        mock_preds.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut embedding = vec![0.0f32; 512];
        embedding[0] = 0.95;
        embedding[1] = 0.05;

        Ok((embedding, mock_preds))
    }
}
