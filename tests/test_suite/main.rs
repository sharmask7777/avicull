//! # Consolidated Integration Test Runner
//!
//! This crate aggregates all integration and spec conformance tests located in `tests/test_suite/`
//! into a single compiled integration test binary (`test_suite`).
//!
//! ## Architectural Benefits
//! - **Eliminates Mach-O Link Contention**: Instead of compiling 38+ separate Mach-O binaries via `ld64`,
//!   all integration tests are linked into a single binary, reducing incremental test link times
//!   from >44s down to <5s and saving >5.8 GB of target directory bloat.
//! - **Standard Module Resolution**: Under standard Rust 2021 edition rules for directory targets
//!   (`tests/<target>/main.rs`), sibling files in `tests/<target>/<module>.rs` resolve naturally
//!   as standard submodules without any manual `#[path = "..."]` annotations.
//! - **Preserves Standalone E2E Targets**: Standalone test targets (`test_mock_directory_structure_e2e.rs`
//!   and `test_e2e_real_samples.rs`) remain decoupled to allow standalone environment and binary path overrides.

mod test_burst;
mod test_catalog;
mod test_catalog_html_generation;
mod test_catalog_snapshot;
mod test_chunk_processing;
mod test_classifier;
mod test_clean_rollback;
mod test_cli_correct;
mod test_cli_create_mock_dataset;
mod test_cli_e2e;
mod test_cli_location;
mod test_cloud_resilience;
mod test_dataset_generator;
mod test_db;
mod test_delta_catalog_thumbnail_cache;
mod test_detector;
mod test_diversity_classification;
mod test_e2e_location_workflow;
mod test_e2e_mock_pipeline;
mod test_gazetteer;
mod test_geo;
mod test_help;
mod test_inplace_catalog;
mod test_inplace_rename;
mod test_location_precedence;
mod test_location_resolver;
mod test_organizer;
mod test_path_parser;
mod test_portable_deserialization;
mod test_property_invariants;
mod test_reconcile_moves;
mod test_regional_prior;
mod test_scanner;
mod test_spec_conformance;
mod test_storage_backend;
mod test_subdirectory_manifest_inheritance;
mod test_types;
mod test_update;
