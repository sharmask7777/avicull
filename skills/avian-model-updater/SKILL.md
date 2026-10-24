---
name: avian-model-updater
description: Instructions and operational procedures for an AI coding agent to upgrade avicull's bird classifiers, taxonomies, SINR spatio-temporal prior models, and detection pipelines to current cutting-edge architectures.
---

# Avian Model & Classifier Updater Skill

This document instructs an AI coding agent on how to continuously upgrade the machine learning models, visual classifiers, spatial-temporal priors, and avian taxonomies in `avicull` to remain on the current cutting edge.

---

## 1. Architecture Overview & Model Components

`avicull` employs a decoupled two-tier architecture:
1. **One-Time Python Exporter (`scripts/export_models.py`)**: Uses Python and `uv` to download pre-trained checkpoints, perform PyTorch model tracing, and export static `.onnx` graphs and precomputed `.safetensors` embedding matrices.
2. **Production Rust Engine (`src/`)**: Pure Rust runtime executing ONNX graphs on Apple Silicon (ANE & Metal GPU) and running Bayesian posterior calculations via `candle`.

The system relies on four distinct model artifacts stored in the `models/` directory:

| Model / Artifact | Current Baseline | Cutting-Edge Alternatives | File Name |
|:---|:---|:---|:---|
| **Bird Bounding Box Detector** | Ultralytics YOLO11m (`yolo11m.onnx`) | YOLO11x, RT-DETR, Owl-ViT v2 | `models/yolo11m.onnx` |
| **Visual Feature Extractor** | BioCLIP 2 (ViT-B/16) | BioCLIP 2.5 (ViT-H/14), OSEA MetaFGNet, BioCLIP-ResNet50 | `models/bioclip_vision.onnx` |
| **Avian Taxonomy & Embeddings** | AviList / Clements 2024 (10,900+ species) | IOC World Bird List 14.x, eBird 2025/2026 Taxonomy | `models/taxonomy.json`, `models/species_embeddings.safetensors` |
| **Spatio-Temporal Prior** | SINR (Cole et al., ICML 2023) | Location-Encoder v2, eBird Status & Trends GeoPrior | `models/sinr_geoprior.onnx` |

---

## 2. Step-by-Step Procedure for Updating Models

When tasked with updating `avicull`'s models or taxonomies, the agent MUST follow these sequential steps:

### Phase 1: Research Cutting-Edge Weights & Checkpoints
1. **Visual Classifiers**:
   - Check Hugging Face for the latest releases from [imageomics](https://huggingface.co/imageomics) (e.g. `imageomics/bioclip-2.5-vith14` or `imageomics/bioclip-2`).
   - Check [sun-jiao/osea](https://github.com/sun-jiao/osea) for MetaFGNet species classification checkpoints trained on 10,000+ birds.
2. **Taxonomy & Regional Checklists**:
   - Verify if Cornell Lab of Ornithology released an updated eBird Taxonomy or Clements Checklist.
   - Ensure the JSON format in `models/taxonomy.json` maintains the required schema:
     ```json
     {
       "paisto1": {
         "common_name": "Painted Stork",
         "scientific_name": "Mycteria leucocephala",
         "folder_slug": "Painted_Stork"
       }
     }
     ```
3. **Location Priors**:
   - Inspect [SINR GitHub](https://github.com/elijahcole/sinr) or iNaturalist spatial prior weights.

### Phase 2: Updating the Exporter (`scripts/export_models.py`)
1. Open `scripts/export_models.py` and locate model download definitions.
2. Update the target Hugging Face model IDs, default URLs, or model architecture wrappers.
3. Verify that ONNX export flags use:
   - `opset_version=17` or higher for Apple Neural Engine (CoreML EP) compatibility.
   - `do_constant_folding=True` to minimize graph complexity.
   - Dynamic axes only if necessary; fixed input shapes (e.g., `1x3x224x224`) are preferred for ANE compilation.
4. Execute the exporter in a managed virtual environment:
   ```bash
   cd scripts
   uv run python export_models.py --output-dir ../models/
   ```
5. Confirm all files are generated in `models/`:
   - `yolo11m.onnx`
   - `bioclip_vision.onnx`
   - `sinr_geoprior.onnx`
   - `species_embeddings.safetensors`
   - `taxonomy.json`
   - `manifest.json`

### Phase 3: Updating Rust Domain Models & Math
1. **Embedding Dimensionality**:
   If switching from ViT-B (512-dim) to ViT-H (1024-dim):
   - Update embedding vectors in [`src/classifier.rs`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/src/classifier.rs) and [`src/burst.rs`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/src/burst.rs) (`vec![0.0; 1024]`).
2. **Temperature Scaling ($\tau$) & Prior Weights ($\alpha$)**:
   - BioCLIP ViT-H outputs have different logit ranges. Calibrate the default `--geo-prior-weight` in `src/main.rs` if needed to maintain optimal balance between visual confidence and geographical probability.
3. **Species Slug Sanitization**:
   - Ensure any new species in `taxonomy.json` pass through `sanitize_species_slug()` cleanly without invalid filesystem characters.

---

## 3. Mandatory Verification Loop

Before concluding any model update, the agent MUST run the following multi-pronged verification:

1. **Synthetic Unit & Integration Tests**:
   ```bash
   cargo test
   ```
   Verify that all 27+ tests pass, including:
   - `test_posterior_downranks_out_of_range_lookalike`
   - `test_burst_consensus_smoothing_5_frames`
   - `test_spec_conformance`
2. **Lint & Code Hygiene**:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   ```
3. **Real-World Leh Dataset Smoke Test**:
   ```bash
   cargo build --release
   target/release/avicull organize --input "/Users/shaleensharma/Pictures/Leh" --models-dir models/ --dry-run
   ```
   Verify that:
   - Inference completes without panic or tensor shape mismatches.
   - Accuracy metrics on known Leh species (Tibetan Snowcock, Brown-headed Gull, etc.) show expected confidence.
4. **Dispatch Agent Crew Review**:
   - Spawn subagents to verify safety, immutability invariants, and spec compliance.
