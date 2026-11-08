---
title: "Decoupling Non-Bird Asset Categories and Resolving Substring Clashes in Photographic Genres"
date: "2026-11-08"
category: "logic-errors"
module: "taxonomy"
problem_type: "logic_error"
component: "data_model"
severity: "high"
symptoms:
  - "Avian species with animal or plant names (e.g. European Turtle-Dove, Snake-Eagle, Lizard-Cuckoo) falsely classified as Mammalia or Reptilia"
  - "Unclassified and blurry frames forced into Bird hierarchy rather than a neutral fallback tier"
  - "Non-animal photographic genres (Scenery, Architecture, Portrait, Abstract) returning zero matches during embedding inference"
root_cause: "logic_error"
resolution_type: "code_fix"
tags:
  - "asset-category"
  - "multi-taxa"
  - "substring-collisions"
  - "photographic-genres"
  - "nosubjectdetected"
---

# Decoupling Non-Bird Asset Categories and Resolving Substring Clashes in Photographic Genres

## Problem
In early architecture designs, `avicull` assumed every scanned image was fundamentally an avian photograph. As functionality expanded to handle diverse wildlife (Mammalia, Insecta, Reptilia, Amphibia) and photographic genres (Scenery, Architecture, Portrait, Abstract):
1. **Fuzzy Substring Clashes**: The category router attempted to match common name substrings (e.g. `"Turtle"`, `"Snake"`, `"Lizard"`, `"Fox"`). This caused legitimate bird species like European Turtle-Dove, Snake-Eagle, Lizard-Cuckoo, Bearded Guan, and Desert Lark to be misrouted into non-avian categories like `Reptilia` or `Mammalia`.
2. **Coupling Unclassified Frames to Birds**: Zero-detection frames or empty scenery were treated as `_No_Bird_Detected`, polluting the bird hierarchy with non-bird rejects.
3. **Genre Embedding Centroid Dilution**: Broad photographic genres (such as landscape scenery or architecture) lacked discrete bounding boxes, causing YOLO detector fallbacks to return zero matches or default to arbitrary animal classes.

## Symptoms
- Bird photos of Crested Serpent-Eagle or Turtle-Doves appeared in `birds_hierarchy` under Reptilia instead of Bird.
- Blurry non-subject landscape frames were tagged as unclassified birds.
- People and buildings photographed on trips failed to match any taxonomic category.

## What Didn't Work
- Expanding substring heuristic rules: Adding more substring exclusions (e.g. `if slug.contains("dove")`) became an unmaintainable game of taxonomic whack-a-mole across thousands of species.
- Falling back directly to the highest global embedding cosine similarity without confidence gating: Low-confidence foliage was frequently misclassified into non-avian genres.

## Solution
1. **Exact Curation via Curated Multi-Taxa Manifest**:
   Replace fuzzy substring matching with an exact lookup against `data/curated_multi_taxa.json`. The curated database explicitly registers non-avian species. If a taxon is not in `data/curated_multi_taxa.json`, it cleanly resolves as `AssetCategory::Bird`.

2. **Introduction of `AssetCategory::NoSubjectDetected`**:
   In `src/types.rs`, decouple empty and unclassified frames from `Bird` by establishing `AssetCategory::NoSubjectDetected` routing to `_No_Subject_Detected`:

   ```rust
   // src/types.rs
   pub enum AssetCategory {
       Bird,
       Mammalia,
       Insecta,
       Reptilia,
       Amphibia,
       Scenery,
       Architecture,
       Portrait,
       Abstract,
       NoSubjectDetected,
       Other(String),
   }
   ```

3. **Detector Class Routing and Top-2 Pooling**:
   In `scripts/infer.py`, add COCO person class ID 0 to the detection animal set to route human subjects directly to `Portrait`. For non-animal frames, apply top-2 score pooling across genre embeddings (Scenery, Architecture, Abstract) with distinct mean and peak threshold gates. Ambiguous frames below confidence thresholds fall back cleanly to `_No_Subject_Detected`.

## Why This Works
- Exact matching ensures that avian species with deceptive names (e.g., European Turtle-Dove, Snake-Eagle) are never mistakenly categorized as reptiles or mammals.
- `AssetCategory::NoSubjectDetected` provides a neutral tier for out-of-focus or subject-less assets without implying an avian subject.
- Dual-gated top-2 pooling prevents centroid dilution when classifying open landscapes and architecture.

## Prevention
- Unit tests in `tests/test_suite/test_catalog.rs` and `tests/test_suite/test_extensible_taxa_guardrails.rs` test 14 specific bird species with animal/plant substrings to guarantee they resolve as `AssetCategory::Bird`.
- Invariant 2 in `AGENTS.md` Section 7 mandates generic, data-driven biodiversity taxonomies over ad-hoc heuristics.

## Related Issues
- Solved in PR #46, PR #47, and PR #48
- Tested in `tests/test_suite/test_catalog.rs` and `tests_python/test_infer.py`
