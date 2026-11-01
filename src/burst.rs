//! # Burst Consensus Clustering & Temporal Confidence Pooling
//!
//! Avian wildlife photography is characterized by high-speed continuous shooting (bursts of 10–30 frames/sec).
//! Within a burst, individual frames suffer from motion blur, wing occlusions, or extreme angles where
//! a neural classifier's single-frame confidence may dip, even though adjacent frames provide crystal-clear views
//! of the exact same bird.
//!
//! This module clusters consecutive captures into cohesive temporal bursts and applies confidence-weighted
//! Bayesian consensus pooling across all frames in the burst.
//!
//! ## Mathematical Algorithm
//!
//! 1. **Temporal & Semantic Burst Partitioning**:
//!    Captures are grouped within each trip folder by timestamp $\Delta t$. Two consecutive frames $I_m$ and $I_{m+1}$
//!    belong to the same burst if and only if:
//!    $$\Delta t = |t_{m+1} - t_m| \le \text{burst\_window\_ms} \quad \text{AND} \quad \text{sim}(e_m, e_{m+1}) \ge \text{min\_similarity}$$
//!    where $\text{sim}(e_m, e_{m+1}) = \frac{e_m \cdot e_{m+1}}{\|e_m\| \|e_{m+1}\|}$ is the cosine similarity
//!    between normalized deep image embeddings.
//!
//! 2. **Confidence-Weighted Posterior Consensus**:
//!    Rather than taking an unweighted average (which could allow blurry frames to corrupt clear ones),
//!    each frame's species posterior distribution $P(s | I_m)$ is weighted by its bird detector confidence $w_m$:
//!    $$w_m = \max(0.01, \text{detection\_confidence}(I_m))$$
//!    $$\bar{P}(s) = \frac{\sum_{m=1}^M w_m \cdot P(s | I_m)}{\sum_{m=1}^M w_m}$$
//!
//! 3. **Consensus Winner Assignment**:
//!    The consensus winning species is $s^* = \arg\max_s \bar{P}(s)$ with score $C^* = \bar{P}(s^*)$.
//!    Depending on [`ConfidenceMode`], either the consensus score $C^*$ or the per-frame posterior $P(s^* | I_m)$
//!    is recorded on each bundle decision.

use crate::types::{BundleDecision, ConfidenceMode, RoutingTier, SpeciesPrediction};
use std::collections::HashMap;

/// An intermediate representation of an image bundle with neural embeddings and posteriors ready for burst smoothing.
#[derive(Debug, Clone)]
pub struct ClassifiedBundleItem {
    /// Unique BLAKE3 hash identifying the bundle.
    pub bundle_id: String,
    /// Name of the trip folder or parent directory to prevent cross-trip burst clustering.
    pub trip_folder: String,
    /// UTC capture epoch in milliseconds from EXIF `DateTimeOriginal`.
    pub capture_timestamp_ms: Option<i64>,
    /// Avian detection confidence from YOLO detector in `[0.0, 1.0]`.
    pub detection_confidence: f32,
    /// Normalized bounding box `[ymin, xmin, ymax, xmax]`.
    pub bbox_norm: Option<[f32; 4]>,
    /// High-dimensional visual feature embedding vector (e.g. 512-dim or 768-dim).
    pub embedding: Vec<f32>,
    /// Top species predictions sorted descending.
    pub top_predictions: Vec<SpeciesPrediction>,
    /// Full posterior probability vector as pairs of `(taxon_id, posterior_probability)`.
    pub posteriors: Vec<(String, f32)>,
}

/// Computes the cosine similarity between two feature vectors in $[-1.0, 1.0]$.
///
/// Returns `0.0` if vector dimensions differ or vector magnitudes are near zero ($< 10^{-12}$).
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom <= 1e-12 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

/// Clusters classified bundles into temporal bursts and calculates confidence-weighted consensus pooling.
///
/// # Arguments
/// * `items` - Slices of classified bundles with timestamps and embedding vectors.
/// * `burst_window_ms` - Maximum time delta between frames in milliseconds (typically 3,000 ms for 3-second bursts).
/// * `min_similarity` - Minimum cosine similarity required to merge adjacent frames into the same burst (e.g. 0.80).
/// * `confidence_mode` - Whether to assign the pooled burst consensus score or individual per-frame score to each bundle.
/// * `min_confidence` - Threshold for routing to [`RoutingTier::ConfirmedSpecies`] vs [`RoutingTier::NeedsReview`].
///
/// # Invariants
/// * Bundles in different `trip_folder` partitions are never clustered together.
/// * The output vector preserves the exact original item ordering of `items`.
pub fn smooth_burst_consensus(
    items: &[ClassifiedBundleItem],
    burst_window_ms: i64,
    min_similarity: f32,
    confidence_mode: ConfidenceMode,
    min_confidence: f32,
) -> Vec<BundleDecision> {
    if items.is_empty() {
        return Vec::new();
    }

    // Partition by trip folder (using BTreeMap for deterministic trip ordering)
    let mut trip_map: std::collections::BTreeMap<String, Vec<&ClassifiedBundleItem>> =
        std::collections::BTreeMap::new();
    for item in items {
        trip_map
            .entry(item.trip_folder.clone())
            .or_default()
            .push(item);
    }

    let mut decisions: Vec<BundleDecision> = Vec::with_capacity(items.len());
    let mut burst_counter = 0usize;

    for (_trip, mut trip_items) in trip_map {
        // Sort items by capture_timestamp_ms
        trip_items.sort_by_key(|it| it.capture_timestamp_ms.unwrap_or(0));

        // Cluster into bursts
        let mut bursts: Vec<Vec<&ClassifiedBundleItem>> = Vec::new();
        let mut current_burst: Vec<&ClassifiedBundleItem> = Vec::new();

        for &item in &trip_items {
            if current_burst.is_empty() {
                current_burst.push(item);
                continue;
            }

            let prev = *current_burst.last().unwrap();
            let dt = match (item.capture_timestamp_ms, prev.capture_timestamp_ms) {
                (Some(t1), Some(t0)) => (t1 - t0).abs(),
                _ => i64::MAX,
            };

            let sim = cosine_similarity(&item.embedding, &prev.embedding);

            if dt <= burst_window_ms && sim >= min_similarity {
                current_burst.push(item);
            } else {
                bursts.push(current_burst);
                current_burst = vec![item];
            }
        }

        if !current_burst.is_empty() {
            bursts.push(current_burst);
        }

        // Process each burst
        for burst in bursts {
            burst_counter += 1;
            let burst_id = format!("burst-{:04}", burst_counter);

            // Confidence-weighted pooling across burst frames
            // P_bar(s) = sum(w_m * P_post(s | I_m)) / sum(w_m)
            let mut weighted_sums: HashMap<String, f32> = HashMap::new();
            let mut species_slugs: HashMap<String, String> = HashMap::new();
            let mut weight_total = 0.0f32;

            for item in &burst {
                let w = item.detection_confidence.max(0.01);
                weight_total += w;

                for (taxon_id, prob) in &item.posteriors {
                    *weighted_sums.entry(taxon_id.clone()).or_insert(0.0) += w * prob;
                }
                for pred in &item.top_predictions {
                    species_slugs
                        .entry(pred.taxon_id.clone())
                        .or_insert_with(|| pred.folder_slug.clone());
                }
            }

            // Find consensus winning species
            let mut best_species = String::new();
            let mut best_consensus_score = -1.0f32;

            if weight_total > 0.0 {
                for (taxon_id, sum_val) in &weighted_sums {
                    let score = sum_val / weight_total;
                    if score > best_consensus_score {
                        best_consensus_score = score;
                        best_species = taxon_id.clone();
                    }
                }
            }

            let assigned_slug = species_slugs
                .get(&best_species)
                .cloned()
                .unwrap_or_else(|| {
                    burst[0]
                        .top_predictions
                        .first()
                        .map(|p| p.folder_slug.clone())
                        .unwrap_or_else(|| "Unknown_Bird".to_string())
                });

            // Build decisions for each frame in the burst
            for item in &burst {
                let frame_conf = item
                    .top_predictions
                    .first()
                    .map(|p| p.combined_score)
                    .unwrap_or(0.0);

                let per_frame_for_winner = item
                    .posteriors
                    .iter()
                    .find(|(id, _)| id == &best_species)
                    .map(|(_, p)| *p)
                    .unwrap_or(0.0);

                let final_conf = match confidence_mode {
                    ConfidenceMode::BurstConsensus => best_consensus_score.max(0.0),
                    ConfidenceMode::PerFrame => per_frame_for_winner.max(0.0),
                };

                let routing_tier = if item.detection_confidence < 0.25 {
                    RoutingTier::NoBirdDetected
                } else if final_conf >= min_confidence {
                    RoutingTier::ConfirmedSpecies
                } else {
                    RoutingTier::NeedsReview
                };

                decisions.push(BundleDecision {
                    bundle_id: item.bundle_id.clone(),
                    burst_id: Some(burst_id.clone()),
                    detection_confidence: item.detection_confidence,
                    bbox_xyxy_norm: item.bbox_norm,
                    top_predictions: item.top_predictions.clone(),
                    frame_confidence: frame_conf,
                    final_confidence: final_conf,
                    assigned_species_slug: assigned_slug.clone(),
                    routing_tier,
                    is_user_override: false,
                });
            }
        }
    }

    // Preserve original bundle order from input items
    let mut decision_map: HashMap<String, BundleDecision> = HashMap::new();
    for d in decisions {
        decision_map.insert(d.bundle_id.clone(), d);
    }

    items
        .iter()
        .filter_map(|it| decision_map.remove(&it.bundle_id))
        .collect()
}
