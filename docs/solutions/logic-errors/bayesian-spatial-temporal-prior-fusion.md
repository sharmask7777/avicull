---
title: "Eliminating Cross-Continental False Positives via Bayesian Spatial-Temporal Prior Fusion"
date: "2026-11-08"
category: "logic-errors"
module: "classifier"
problem_type: "logic_error"
component: "service_layer"
severity: "high"
symptoms:
  - "Computer vision models predicting North American bird species on photos taken in the Himalayas"
  - "Visually similar sister taxa (e.g. American Goldfinch vs European Greenfinch) misclassified outside their native ranges"
  - "High visual confidence scores overriding known geographic distributions"
root_cause: "logic_error"
resolution_type: "code_fix"
tags:
  - "bayesian-posterior"
  - "spatial-priors"
  - "ebird-checklists"
  - "haversine-distance"
---

# Eliminating Cross-Continental False Positives via Bayesian Spatial-Temporal Prior Fusion

## Problem
Neural vision classifiers trained on global bird biodiversity output uncalibrated visual probabilities $P_{\text{vis}}(s \mid I)$ across thousands of species. When presented with sub-optimal lighting, heavy plumage molting, or distant compositions, visual backbones often predict geographically impossible species (e.g., predicting an African or Nearctic bird species for an image captured in Ladakh or the Western Ghats of India) with spuriously high visual confidence (>85%).

## Symptoms
- Out-of-range false positives dominating classification results despite accurate EXIF GPS tags or clear trip directory names (e.g. `Leh`).
- Sister taxa that share nearly identical plumage morphology confusing the visual classifier.
- Lack of probabilistic discounting for species never observed within a given biogeographical zone.

## What Didn't Work
- Hard whitelist filtering: Completely discarding any candidate species not in a static regional list caused true vagrant sightings, unusual migratory records, or recent taxonomical splits to fail with unclassified errors.
- Simple heuristic score subtraction: Arbitrary penalty values caused calibration drift across different camera sensors and confidence thresholds.

## Solution
Implement a mathematically grounded Bayesian posterior fusion mechanism combining raw visual logits with spatial-temporal priors derived from embedded gazetteers and regional checklists.

In `src/classifier.rs`:

1. **Location Resolution**:
   - Primary: EXIF GPS coordinates are matched against `data/gazetteer.json` waypoints using Haversine spherical distance within a territorial radius.
   - Fallback: Directory path tokens (e.g. `Leh`, `K Gudi`) are parsed by `src/location/path_parser.rs` with path-noise filtering to infer regional administrative codes.

2. **Bayesian Formulation**:
   Visual probabilities are fused with regional checklist occurrence $P_{\text{geo}}(s \mid \ell, t)$ weighted by geographic factor $\alpha$:

   $$P_{\text{unnorm}}(s \mid I, \ell, t) = P_{\text{vis}}(s \mid I) \cdot \left[\max(P_{\text{geo}}(s \mid \ell, t), \epsilon)\right]^\alpha$$

   $$P_{\text{post}}(s \mid I, \ell, t) = \frac{P_{\text{unnorm}}(s \mid I, \ell, t)}{\sum_{s'} P_{\text{unnorm}}(s' \mid I, \ell, t)}$$

   where $\epsilon = 0.01$ ensures vagrants and rare records retain a non-zero floor.

3. **Regional Attenuation**:
   When an ISO 3166-2 regional code is resolved, candidate taxa absent from the regional checklist in `data/regional_checklists.json` receive an attenuation factor of $10^{-3}$ ($0.001$), while native taxa maintain a $1.0\times$ multiplier.

## Why This Works
The Bayesian formulation attenuates non-native species while leaving genuinely extraordinary visual evidence (e.g., visual probability $>0.999$) capable of overcoming the prior floor $\epsilon$. It cleanly separates visual perception from biogeographic likelihood without fragile hard-coding.

## Prevention
- All location inferences and checklist priors are driven by `data/gazetteer.json` and `data/regional_checklists.json` rather than ad-hoc code heuristics.
- Unit and integration tests in `tests/test_suite/test_gazetteer.rs` and `tests/test_suite/test_location_precedence.rs` verify location precedence and prior scaling.

## Related Issues
- Solved in PR #44 and PR #45
- Formulations documented in `src/classifier.rs` and `src/location/mod.rs`
