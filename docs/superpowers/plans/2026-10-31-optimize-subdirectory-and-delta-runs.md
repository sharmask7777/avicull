# Optimize Subdirectory Executions and Delta Re-runs

This plan addresses [Issue #18: optimize subdirectory executions](https://github.com/sharmask7777/avicull/issues/18):
1. **Subdirectory Manifest Re-use**: When subdirectories have had their own `avicull organize` run (having an existing `.avicull/manifest.sqlite`), the parent run re-uses that context instead of rebuilding from scratch.
2. **Fast Delta Runs via SQLite Micro-Thumbnail Cache**: `generate_html_catalog` previously spent significant time per run re-generating and downsampling micro-thumbnails for thousands of items. Caching base64 micro-thumbnails directly in SQLite enables instant delta updates in seconds.
3. **Subdirectory Manifest Merging**: Automatically discovers and imports child manifests into the parent manifest when scanning.

---

### Task 1: Micro-Thumbnail Persistence in SQLite for Instant Delta Catalog Runs
- **Goal**: Add a `bundle_thumbnails` table in `ManifestDb` (storing `bundle_id`, `data_uri`, `updated_at`). In `generate_html_catalog`, check the DB cache before extracting/downsampling from disk, and persist newly generated thumbnails in batch.
- **TDD Test**: `tests/test_delta_catalog_thumbnail_cache.rs`
- **Implementation**:
  - `src/db.rs`: Add table `bundle_thumbnails` with schema and methods `get_thumbnail_data_uri(&self, bundle_id: &str)`, `get_all_thumbnails(&self)`, and `store_thumbnails(&self, items: &[(&str, &str)])`.
  - `src/catalog/mod.rs`: Accept optional reference to `ManifestDb` or thumbnail map in `generate_html_catalog` / `CatalogPayload`, re-using cached data URIs without image re-compression.

### Task 2: Subdirectory Manifest Discovery & Import
- **Goal**: When organizing `--input <DIR>`, scan for child manifests in subdirectories (e.g. `<DIR>/<subfolder>/.avicull/manifest.sqlite` or `<DIR>/<subfolder>/birds_hierarchy/.avicull/manifest.sqlite`). Merge their bundle decisions, predictions, and locations into the parent manifest so the parent run treats them as pre-cached bundles.
- **TDD Test**: `tests/test_subdirectory_manifest_inheritance.rs`
- **Implementation**:
  - `src/scanner.rs` & `src/db.rs`: Add `discover_child_manifests(input_dir: &Path)` and `import_child_manifest(&self, child_db_path: &Path, rel_subdir: &Path)`.
  - `src/main.rs`: Before partitioning into new vs cached bundles in `execute_organize`, discover and import any child manifests into the active parent manifest.

### Task 3: End-to-End Verification & Real Samples Delta Benchmark
- **Goal**: Verify that adding a subdirectory or re-running on an existing tree completes in seconds with zero redundant ML inference or thumbnail regeneration.
- **Verification**: Run `cargo test` across all suites, test formatting and lints (`cargo fmt`, `cargo clippy -- -D warnings`), and push PR.
