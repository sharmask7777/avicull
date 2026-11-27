# Task 1 Implementation Report: Self-Contained Inference Script and Model Assets Resolution

## Overview

**Status:** DONE  
**Commit:** `aaafc2d36cbcf7ed2524af8a7ce0393073923986` (`feat(inference): resolve and extract self-contained inference runner and checklists`)  
**Covering Tests:** `tests/test_suite/test_standalone_inference_resolution.rs` (10 tests), `test_suite` integration tests (331 tests total), `tests/test_mock_directory_structure_e2e.rs`, and `./scripts/verify_python.sh` (85 tests, 94.45% coverage).

---

## Changes Implemented

### 1. Embedded Payload Constants in `src/classifier.rs`
- Embedded `scripts/infer.py` into binary payloads using `pub const EMBEDDED_INFERENCE_SCRIPT: &str = include_str!("../scripts/infer.py");`.
- Exposed `data/regional_checklists.json` as `pub const EMBEDDED_REGIONAL_CHECKLISTS_JSON: &str = include_str!("../data/regional_checklists.json");`.

### 2. Standalone Inference Script Resolution (`src/classifier.rs`)
- Implemented `resolve_inference_script() -> Result<PathBuf>` and parameterized testable helper `resolve_inference_script_in(cwd: &Path, exe_dir: Option<&Path>, cache_dir: &Path) -> Result<PathBuf>`.
- Prioritized candidate resolution:
  1. `./scripts/infer.py` (relative to CWD for active local repository development).
  2. `<current_exe_dir>/scripts/infer.py` (next to binary in standalone application distributions).
  3. `~/.cache/avicull/scripts/infer.py` (or `$AVICULL_CACHE_DIR/scripts/infer.py` / temp directory fallback).
- Cache extraction guarantees:
  - Compares existing file content using `blake3` hash.
  - If missing or if hash differs, extracts `EMBEDDED_INFERENCE_SCRIPT` atomically via a PID-tagged temp file and rename.
  - On Unix, sets executable permissions (`0o755`).

### 3. Regional Checklist Syncing (`src/classifier.rs` & `src/main.rs`)
- Implemented `sync_regional_checklists(models_dir: &Path) -> Result<PathBuf>` to ensure `<models_dir>/regional_checklists.json` exists and is non-empty, extracting `EMBEDDED_REGIONAL_CHECKLISTS_JSON` atomically if missing or empty.
- Updated `resolve_or_prompt_models_dir` in `src/main.rs` to automatically call `sync_regional_checklists(m_dir)` whenever a models directory is resolved (whether explicitly requested via `--models-dir`, found in `./models`, located in `~/.cache/avicull/models`, or freshly initialized).

### 4. Decoupling Hardcoded `scripts/infer.py` in `src/main.rs`
- In `run_batch_inference`, replaced `Path::new("scripts/infer.py")` with `resolve_inference_script()?`, passing the resolved path dynamically to the Python child process command line.
- In the chunk organization loop (`Commands::Organize`), replaced `Path::new("scripts/infer.py").exists()` with `resolve_inference_script().is_ok_and(|p| p.is_file())`.

---

## Test-Driven Development (TDD) Progression

1. **RED Phase**:
   - Authored 10 targeted unit and integration tests in `tests/test_suite/test_standalone_inference_resolution.rs`.
   - Exposed module in `tests/test_suite/main.rs`.
   - Verified expected compile failure (`E0432: unresolved imports`, `E0603: static EMBEDDED_REGIONAL_CHECKLISTS_JSON is private`).

2. **GREEN Phase**:
   - Implemented constants, `resolve_inference_script`, `resolve_inference_script_in`, and `sync_regional_checklists` in `src/classifier.rs`.
   - Integrated resolution and checklist extraction in `src/main.rs`.
   - Ran `cargo test --test test_suite test_standalone_inference`: all 10 tests passed in 0.00s.

3. **REFACTOR & Verification Phase**:
   - Applied `cargo fmt`.
   - Resolved clippy pedantic suggestion (`is_ok_and` instead of `map.unwrap_or(false)`).
   - `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings.
   - `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: 0 warnings.
   - `cargo test --test test_suite`: 331 tests passed.
   - `cargo test --test test_mock_directory_structure_e2e`: passed.
   - `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok.
   - `./scripts/verify_python.sh`: ruff passed, mypy passed, 85 pytest tests passed with 94.45% coverage ($\ge 80\%$).

---

## Covering Tests

1. `test_embedded_assets_are_non_empty_and_valid`
2. `test_resolve_inference_script_from_cwd`
3. `test_resolve_inference_script_from_exe_dir`
4. `test_resolve_inference_script_extracts_to_cache_when_missing`
5. `test_resolve_inference_script_updates_stale_cache_when_hash_differs`
6. `test_resolve_inference_script_preserves_valid_cache`
7. `test_sync_regional_checklists_extracts_when_missing`
8. `test_sync_regional_checklists_overwrites_empty_file`
9. `test_sync_regional_checklists_preserves_existing_custom_content`
10. `test_resolve_inference_script_live`

---

## Concerns / Regressions

- None. All source immutability invariants and existing CLI/pipeline workflows continue to pass without regression.
