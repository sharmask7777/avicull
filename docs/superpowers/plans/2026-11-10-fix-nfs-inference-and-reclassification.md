# Standalone Inference Resolution and Re-classification of Unclassified/Location-Updated Bundles

## Context & Problem Statement
When running `avicull organize --input <PATH>` on an external/NFS mount outside the repository:
1. `src/main.rs` uses a hardcoded relative path `Path::new("scripts/infer.py")` to locate the inference script. Outside the repo, this path does not exist, causing `avicull` to silently fall back to `run_fallback_parallel_classification`.
2. In fallback mode, `OrtBirdDetector` looks for `yolo11m.onnx`, which is absent in `~/.cache/avicull/models/` (which only contains `yolo11n.pt` and `species_embeddings.safetensors`). It falls back to `MockBirdDetector::new(vec![])`, which yields 0 detections and assigns `detection_confidence = 0.0` and `_No_Subject_Detected` to all 906 bundles.
3. Once written to SQLite, subsequent runs check `existing_bundle_map`. Because `new_bundles` is empty, `avicull` early-returns with `✓ All photo bundles are already up to date`, even if the user subsequently updates coordinates in `catalog.html` (`directory_locations`).
4. Standalone Python inference also lacked `regional_checklists.json` in `~/.cache/avicull/models/` when running outside the repo.

## Implementation Tasks

### Task 1: Self-Contained Inference Script and Model Assets Resolution
- Embed `scripts/infer.py` into the binary via `include_str!("../scripts/infer.py")`.
- Implement `resolve_inference_script() -> Result<PathBuf>`:
  - Check `./scripts/infer.py` (CWD for local dev).
  - Check `<current_exe_dir>/scripts/infer.py` (next to binary).
  - Cache and extract `include_str!` to `~/.cache/avicull/scripts/infer.py` if missing or if hash differs.
- In `resolve_or_prompt_models_dir`:
  - Ensure `regional_checklists.json` is extracted into `~/.cache/avicull/models/regional_checklists.json` from `EMBEDDED_REGIONAL_CHECKLISTS_JSON` if not present.
- Update `run_batch_inference` and the detection pipeline in `src/main.rs` to use `resolve_inference_script()`.
- Add test coverage in `tests/test_suite/test_standalone_inference_resolution.rs`.

### Task 2: Re-evaluation of Mock-Failed and Location-Updated Bundles
- In `src/main.rs`:
  - When evaluating `existing_bundle_map` against `all_discovered`:
    - If `args.reclassify` is true, reclassify all non-override bundles.
    - If a bundle has `detection_confidence == 0.0` and `top_predictions.is_empty()` and `!is_user_override` (i.e. previously failed/mocked without real inference) and real models/inference are available, treat it as requiring classification rather than caching it as a valid decision.
    - If a bundle's resolved `gps_coords` or region differs from the cached bundle coordinates in the database, queue it for re-classification.
  - Add `--reclassify` flag to `avicull organize` in `src/cli.rs`.
- Add test coverage in `tests/test_suite/test_reclassify_unclassified_bundles.rs`.

### Task 3: Verification & Real-World Validation
- Run `./scripts/verify.sh` to ensure clippy, fmt, tests, python, cargo deny, and rustdoc pass cleanly.
- Verify running `avicull` from `/tmp` or another directory can resolve inference and reclassify.
