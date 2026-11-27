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

