---
title: "Remediating Prior Distortion, Low-Detection Avian Hallucinations, and Visual Dominance"
date: "2026-11-16"
category: "logic-errors"
module: "classifier"
problem_type: "logic_error"
component: "service_layer"
severity: "high"
symptoms:
  - "Non-biological objects (museum statues, Roman marble busts, Chinese stone carvings) classified as Confirmed Golden Eagle or Brown Pelican"
  - "Scenic landscapes and waterfalls (e.g. Wallace Falls) misclassified as Confirmed Great Blue Heron"
  - "Native breeding residents (e.g. Pileated Woodpecker in Washington State) suppressed by 1,000x penalty due to incomplete regional checklist"
  - "Captive zoo animals (e.g. Toco Toucan in a Seattle zoo) misclassified as local common birds (Brown Pelican) due to excessive prior attenuation"
  - "Low-mass posterior tail distributions normalized into false 90%+ confidence classifications"
root_cause: "logic_error"
resolution_type: "code_fix"
tags:
  - "bayesian-posterior"
  - "prior-attenuation"
  - "visual-dominance"
  - "relevance-floor"
  - "detector-gating"
  - "ebird-checklists"
---

# Remediating Prior Distortion, Low-Detection Avian Hallucinations, and Visual Dominance

## Problem
In catalog archives containing mixed wildlife, scenery, cultural, and zoo photography, several systematic misclassifications were observed:
1. Museum sculptures, marble busts, and wet waterfall rocks were classified as high-confidence confirmed bird species (`Golden Eagle`, `Brown Pelican`, `Great Blue Heron`).
2. Two consecutive photos of captive Toco Toucans in a Seattle zoo resulted in the first being correctly identified and the second mislabelled as a `Brown Pelican`.
3. High-quality photos of native birds (e.g., `Pileated Woodpecker` in Washington state) were penalized with extreme out-of-region multipliers.

## Symptoms & Root Causes

### 1. Absence of Visual Relevance Floor on Detector Crops
YOLO object detection models occasionally produce weak candidate bounding boxes ($\text{conf} < 0.50$) on high-contrast textures, splashing water, rocks, or carved stone. Previously, the pipeline assumed any detected bounding box contained an animal and forwarded the cropped region to the BioCLIP visual model. In the absence of a visual relevance floor, the cropped patch of rock or marble—despite having near-zero cosine similarity ($\sim 0.20 - 0.24$, baseline noise) across all 11,000+ species—was forced into the species classifier.

### 2. Pathological $1{,}000\times$ Regional Prior Attenuation
In `scripts/infer.py`, candidate species absent from a resolved regional checklist received a multiplier of $0.001$, while in-region species received $1.0$. When an exotic or captive bird (e.g., Toco Toucan in Seattle) was photographed, its high visual probability ($P_{\text{vis}} \approx 0.216$) was crushed by $0.001$ to $0.000216$. Meanwhile, a local bird with tiny background noise probability in the tail ($P_{\text{vis}} \approx 0.0011$) received a $1.0\times$ multiplier. Because $0.0011 > 0.000216$, the local candidate won.

### 3. Uncalibrated Top-5 Posterior Normalization
Once the top-5 candidates were extracted, their scores were normalized by $\sum_{i=1}^5 \text{score}_i$. If the total probability mass captured by the top 5 was tiny (e.g., $0.012$ because the model was diffused across thousands of unrelated classes on a noise crop), dividing by $0.012$ mathematically inflated a $0.010$ noise candidate into an $83\%$ confidence prediction. Furthermore, the final confidence was decoupled from the detector's confidence, allowing a $0.27$ detection to become a $0.80$ confirmed species.

### 4. Checklist Coverage Gaps
The `US-WA` checklist in `data/regional_checklists.json` was missing prominent Pacific Northwest breeding residents, including Pileated Woodpecker (`pilwoo`), Varied Thrush (`varthr`), and Chestnut-backed Chickadee (`chbchi`), subjecting native wild birds to the out-of-region penalty.

### 5. Subdirectory Path Token Masking
When photos were organized under subfolders (e.g., `US 2016/JPEGs/New York/processed`), the location resolver only queried the trip folder root (`US 2016` $\to$ Seattle), masking the subfolder's geographical identity. Furthermore, path climbing traversed up into host system directories (`/private/var/...`).

## What Didn't Work
- Hard whitelisting: Discarding non-checklist candidates completely eliminated the ability to identify captive birds in zoological parks, botanical gardens, and rare vagrant sightings.
- Pure heuristic prior reductions: Lowering the prior weight globally degraded disambiguation between sister taxa in wild birding trips.

## Solution

### 1. Visual Relevance Floor & Genre Fallback (`scripts/infer.py`)
Before forwarding a detected bounding box to fine-grained taxonomy ranking, evaluate its peak cosine similarity against all 11,000+ species centroids:
- If $\text{peak\_sim} < 0.23$ (or $< 0.25$ when $\text{det\_conf} < 0.35$), the crop does not visually resemble any biological taxon.
- The crop is rejected as a detector hallucination, and the pipeline falls back to full-frame classification (`Scenery`, `Architecture`, `Abstract`, or `_No_Subject_Detected`).

### 2. Soft Bounded Prior & Visual Dominance Override (`scripts/infer.py`)
- The out-of-region penalty is bounded at $0.05$ ($20\times$ penalty instead of $1{,}000\times$).
- **Visual Dominance Override**: When the top visual candidate has strong visual evidence ($P_{\text{vis}}(s_{\text{top1}}) \ge 0.15$) and is overwhelmingly higher than any in-region species:
  $$P_{\text{vis}}(s_{\text{top1}}) > 5.0 \times \max_{s \in \text{Region}} P_{\text{vis}}(s)$$
  the geographic prior penalty is bypassed for that candidate ($\text{effective\_prior} = 1.0$). This protects zoo exhibits and vagrants without degrading regional disambiguation.

### 3. Calibrated Normalization & Detection Bounds (`scripts/infer.py`)
- Calibrate the top-5 normalized score by the total captured posterior mass:
  $$\text{calibration\_scale} = \min\left(1.0, \frac{\sum_{i=1}^5 \text{raw\_score}_i}{0.15}\right)$$
- Bound final classification confidence by detector confidence:
  $$\text{final\_conf} = \min(\text{top1\_conf}, \text{det\_conf})$$

### 4. Rust Core Avian Detection Routing Floor (`src/burst.rs`)
In `smooth_burst_consensus`:
- For `AssetCategory::Bird`:
  - $\text{effective\_det} < 0.35 \implies \text{RoutingTier::NoBirdDetected}$
  - $0.35 \le \text{effective\_det} < 0.50 \implies \text{RoutingTier::NeedsReview}$
  - $\text{RoutingTier::ConfirmedSpecies}$ strictly requires $\text{effective\_det} \ge 0.50$ **AND** $\text{final\_conf} \ge \text{min\_confidence}$.
- Non-avian assets (`Mammalia`, `Insecta`, `Scenery`) route to `ConfirmedSpecies` when $\text{final\_conf} \ge \text{min\_confidence}$ regardless of the avian detector confidence.

### 5. Cached Subdirectory Token Resolution & Bounded Tree Traversal (`src/location/`)
- In `src/location/resolver.rs`, parse subfolder path components against `GazetteerEngine::get_embedded()` with score $\ge 0.80$, resolving sub-trip locations (`US-NY`) without trip root masking.
- Path traversal terminates at `is_trip_root`, preventing path climbing into host system directories.
- In `src/location/gazetteer.rs`, cache the embedded engine via `std::sync::OnceLock`, enabling thread-safe static borrowing in $<70\text{ms}$.

### 6. Pacific Northwest Regional Checklist Expansion (`scripts/build_gazetteer.py`)
- Added `PACIFIC_NORTHWEST_TAXON_IDS` (84 native taxa including `pilwoo`, `stejay`, `varthr`, `daejun`, `chbchi`) to `US-WA` in `data/regional_checklists.json`.

## Prevention & Static Guardrails

1. **Rust Guardrails (`tests/test_suite/test_static_architecture_guardrails.rs`)**:
   - `test_guardrail_detection_confidence_routing_floor`: Enforces detection floors for avian routing.
   - `test_guardrail_generic_architecture_no_personal_context_leaks`: Automated AST scan verifying zero personal directory names in core code.
   - `test_guardrail_subdirectory_location_token_resolution`: Validates subdirectory token resolution over parent trip defaults.
2. **Python Guardrails (`tests/test_infer_guardrails.py`)**:
   - `test_guardrail_us_wa_checklist_contains_pileated_woodpecker`: Enforces presence of native taxa.
   - `test_guardrail_prior_bounded_penalty_ratio`: Asserts prior penalty is bounded $\ge 0.02$.
   - `test_guardrail_visual_relevance_floor_rejects_noise_crops`: Asserts non-biological crops are rejected.
   - `test_guardrail_visual_dominance_overrides_prior_for_captive_or_vagrant`: Guarantees captive zoo birds and vagrants are not overridden by local priors.
