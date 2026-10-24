use crate::types::{BundleDecision, ConfidenceMode, RoutingTier, SpeciesPrediction};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ClassifiedBundleItem {
    pub bundle_id: String,
    pub trip_folder: String,
    pub capture_timestamp_ms: Option<i64>,
    pub detection_confidence: f32,
    pub bbox_norm: Option<[f32; 4]>,
    pub embedding: Vec<f32>,
    pub top_predictions: Vec<SpeciesPrediction>,
    pub posteriors: Vec<(String, f32)>, // (taxon_id, posterior)
}

/// Computes cosine similarity between two vectors.
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

/// Groups classified bundles within trip folders into bursts and applies
/// confidence-weighted posterior consensus pooling.
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

    // Partition by trip folder
    let mut trip_map: HashMap<String, Vec<&ClassifiedBundleItem>> = HashMap::new();
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
                    .unwrap_or(frame_conf);

                let final_conf = match confidence_mode {
                    ConfidenceMode::BurstConsensus => best_consensus_score.max(0.0),
                    ConfidenceMode::PerFrame => per_frame_for_winner.max(0.0),
                };

                let routing_tier = if final_conf >= min_confidence {
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
