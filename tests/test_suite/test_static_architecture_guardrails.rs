//! # Static Architecture & Safety Guardrail Tests
//!
//! Enforces permanent architectural invariants and prevents regression across:
//! 1. Directory walker skips for unhidden (`avicull_catalog_files`) and legacy (`.avicull`) catalog dirs.
//! 2. Default species reclassification policy (`ReclassifyMode::Fast`).
//! 3. High-volume dual thumbnail embedding threshold (`ThumbnailEmbeddingPolicy::Auto` -> 2,500 cutoff).
//! 4. View-rendering zero-image-I/O guarantee (presentation decoupled from source assets).

use avicull::catalog::{
    ThumbnailEmbeddingPolicy, CATALOG_FILES_DIR, DEFAULT_CATALOG_FILENAME, LEGACY_CATALOG_FILES_DIR,
};
use avicull::cli::{ReclassifyMode, RenderCatalogArgs};
use avicull::db::{ExistingBundleMap, ManifestDb};
use avicull::organizer::batch::should_skip_directory;
use avicull::organizer::reconcile::{
    reconcile_inplace_manual_renames, reconcile_manual_disk_moves,
};
use avicull::organizer::render_catalog_standalone;
use avicull::scanner::should_skip_dir;
use avicull::types::{AssetBundle, BundleDecision, Classification, RoutingTier, SpeciesPrediction};
use std::fs;
use std::path::Path;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn test_guardrail_directory_walkers_never_omit_catalog_dirs() {
    // Invariant 1: scanner::should_skip_dir MUST skip CATALOG_FILES_DIR and LEGACY_CATALOG_FILES_DIR
    assert!(
        should_skip_dir(CATALOG_FILES_DIR),
        "scanner::should_skip_dir must skip CATALOG_FILES_DIR ({CATALOG_FILES_DIR})"
    );
    assert!(
        should_skip_dir(LEGACY_CATALOG_FILES_DIR),
        "scanner::should_skip_dir must skip LEGACY_CATALOG_FILES_DIR ({LEGACY_CATALOG_FILES_DIR})"
    );
    assert!(
        should_skip_dir("birds_hierarchy"),
        "scanner::should_skip_dir must skip birds_hierarchy output directory"
    );
    assert!(
        should_skip_dir("@eaDir"),
        "scanner::should_skip_dir must skip Synology @eaDir metadata"
    );
    assert!(
        should_skip_dir("Family.photoslibrary"),
        "scanner::should_skip_dir must skip macOS Apple Photos packages"
    );
    assert!(
        !should_skip_dir("2024_05_Kaziranga"),
        "scanner::should_skip_dir must NOT skip legitimate trip subdirectories"
    );

    // Invariant 2: organizer::batch::should_skip_directory MUST skip CATALOG_FILES_DIR and LEGACY_CATALOG_FILES_DIR
    assert!(
        should_skip_directory(CATALOG_FILES_DIR),
        "organizer::batch::should_skip_directory must skip CATALOG_FILES_DIR ({CATALOG_FILES_DIR})"
    );
    assert!(
        should_skip_directory(LEGACY_CATALOG_FILES_DIR),
        "organizer::batch::should_skip_directory must skip LEGACY_CATALOG_FILES_DIR ({LEGACY_CATALOG_FILES_DIR})"
    );
    assert!(
        should_skip_directory("birds_hierarchy"),
        "organizer::batch::should_skip_directory must skip birds_hierarchy"
    );
    assert!(
        should_skip_directory("@eaDir"),
        "organizer::batch::should_skip_directory must skip @eaDir"
    );
    assert!(
        should_skip_directory("Library.photoslibrary"),
        "organizer::batch::should_skip_directory must skip .photoslibrary"
    );
    assert!(
        !should_skip_directory("2023_11_Leh"),
        "organizer::batch::should_skip_directory must NOT skip legitimate trip subdirectories"
    );
}

#[test]
fn test_guardrail_reconcile_walker_skips_catalog_directories() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    // Create an unhidden catalog dir with fake assets
    let unhidden_dir = root.join(CATALOG_FILES_DIR).join("previews");
    fs::create_dir_all(&unhidden_dir).expect("create unhidden catalog dir");
    fs::write(unhidden_dir.join("IMG_0001.jpg"), b"fake preview").expect("write unhidden preview");

    // Create a legacy catalog dir with fake assets
    let legacy_dir = root.join(LEGACY_CATALOG_FILES_DIR).join("previews");
    fs::create_dir_all(&legacy_dir).expect("create legacy catalog dir");
    fs::write(legacy_dir.join("IMG_0002.jpg"), b"legacy preview").expect("write legacy preview");

    // Open a fresh manifest DB
    let db_path = root.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    let db = ManifestDb::open(&db_path).expect("open manifest db");

    let mut bundle_map = ExistingBundleMap::new();

    // 1. Reconcile moved files across root — should encounter ZERO moved/displaced files
    let reconciled_hierarchy = reconcile_manual_disk_moves(root, &db, &mut bundle_map)
        .expect("reconcile hierarchy should succeed");
    assert_eq!(
        reconciled_hierarchy, 0,
        "Reconcile hierarchy walker must skip CATALOG_FILES_DIR and LEGACY_CATALOG_FILES_DIR"
    );

    // 2. Reconcile in-place renames across root — should encounter ZERO moved/displaced files
    let reconciled_inplace = reconcile_inplace_manual_renames(root, &db, &mut bundle_map)
        .expect("reconcile in-place should succeed");
    assert_eq!(
        reconciled_inplace, 0,
        "Reconcile in-place walker must skip CATALOG_FILES_DIR and LEGACY_CATALOG_FILES_DIR"
    );
}

#[test]
fn test_guardrail_reclassify_mode_default_is_fast() {
    // Invariant: Default reclassify mode must be Fast (pure Rust re-scoring from stored visual embeddings)
    // reserving expensive neural network inference (Forced) for explicit opt-in.
    assert_eq!(
        ReclassifyMode::default(),
        ReclassifyMode::Fast,
        "Default ReclassifyMode must be Fast"
    );
}

#[test]
fn test_guardrail_thumbnail_embedding_policy_and_threshold() {
    // Invariant: Default policy is Auto
    assert_eq!(
        ThumbnailEmbeddingPolicy::default(),
        ThumbnailEmbeddingPolicy::Auto,
        "Default ThumbnailEmbeddingPolicy must be Auto"
    );

    // Invariant: Auto resolves to Forced (embedded base64) for <= 2500 photos
    assert_eq!(
        ThumbnailEmbeddingPolicy::Auto.resolve(0),
        ThumbnailEmbeddingPolicy::Forced
    );
    assert_eq!(
        ThumbnailEmbeddingPolicy::Auto.resolve(100),
        ThumbnailEmbeddingPolicy::Forced
    );
    assert_eq!(
        ThumbnailEmbeddingPolicy::Auto.resolve(2500),
        ThumbnailEmbeddingPolicy::Forced
    );
    assert!(ThumbnailEmbeddingPolicy::Auto.should_embed(2500));

    // Invariant: Auto resolves to Suppressed (disk preview links) for > 2500 photos
    assert_eq!(
        ThumbnailEmbeddingPolicy::Auto.resolve(2501),
        ThumbnailEmbeddingPolicy::Suppressed
    );
    assert_eq!(
        ThumbnailEmbeddingPolicy::Auto.resolve(50_000),
        ThumbnailEmbeddingPolicy::Suppressed
    );
    assert!(!ThumbnailEmbeddingPolicy::Auto.should_embed(2501));
    assert!(!ThumbnailEmbeddingPolicy::Auto.should_embed(50_000));

    // Invariant: Suppressed and Forced override counts
    assert_eq!(
        ThumbnailEmbeddingPolicy::Suppressed.resolve(10),
        ThumbnailEmbeddingPolicy::Suppressed
    );
    assert!(!ThumbnailEmbeddingPolicy::Suppressed.should_embed(10));
    assert_eq!(
        ThumbnailEmbeddingPolicy::Forced.resolve(100_000),
        ThumbnailEmbeddingPolicy::Forced
    );
    assert!(ThumbnailEmbeddingPolicy::Forced.should_embed(100_000));
}

#[test]
fn test_guardrail_view_rendering_zero_image_io_and_missing_source_resilience() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    let db_path = root.join(CATALOG_FILES_DIR).join("manifest.sqlite");
    fs::create_dir_all(db_path.parent().expect("parent dir")).expect("create catalog dir");
    let db = ManifestDb::open_or_create(&db_path).expect("open manifest db");

    let run_id = "run-001";
    db.record_run(run_id, root, root, false, "{}")
        .expect("record run in db");

    // Populate manifest with 50 bundles whose primary image files DO NOT EXIST on disk
    for i in 1..=50 {
        let stem = format!("NONEXISTENT_STEM_{i:04}");
        let non_existent_image = root.join("missing_trip").join(format!("{stem}.CR2"));

        let bundle = AssetBundle {
            bundle_id: stem.clone(),
            original_stem: stem.clone(),
            primary_image: non_existent_image,
            trip_folder_name: "missing_trip".to_string(),
            companion_images: Vec::new(),
            sidecar_files: Vec::new(),
            source_inode: 100_000 + i as u64,
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: Some("Kaziranga".to_string()),
            relative_path: None,
        };

        let pred = SpeciesPrediction {
            taxon_id: "grhhor1".to_string(),
            common_name: "Great Hornbill".to_string(),
            scientific_name: "Buceros bicornis".to_string(),
            folder_slug: "Buceros_bicornis_Great_Hornbill".to_string(),
            visual_prob: 0.98,
            geo_prior: 1.0,
            combined_score: 0.98,
        };

        let decision = BundleDecision {
            bundle_id: stem,
            burst_id: None,
            detection_confidence: 0.98,
            bbox_xyxy_norm: None,
            top_predictions: vec![pred.clone()],
            frame_confidence: 0.98,
            final_confidence: 0.98,
            classification: Classification::from_slug(&pred.folder_slug),
            routing_tier: RoutingTier::ConfirmedSpecies,
            is_user_override: false,
        };

        db.record_bundle(run_id, &bundle, &decision)
            .expect("record bundle in db");
    }

    let catalog_path = root.join(DEFAULT_CATALOG_FILENAME);

    // Measure standalone view rendering execution time
    let start = Instant::now();
    let render_args = RenderCatalogArgs {
        input: root.to_path_buf(),
        output_file: Some(catalog_path.clone()),
        trip_name: Some("Test Zero IO Catalog".to_string()),
        mode: None,
        no_thumbnails: false,
    };

    let rendered_path = render_catalog_standalone(&render_args)
        .expect("render_catalog_standalone must succeed with zero image I/O");
    let elapsed = start.elapsed();

    // Guardrails:
    // 1. Must succeed despite 100% of underlying RAW/image files being non-existent on disk
    assert!(
        rendered_path.exists(),
        "Catalog HTML must exist at output path"
    );
    let html = fs::read_to_string(&rendered_path).expect("read catalog HTML");
    assert!(
        html.contains("Great Hornbill"),
        "Catalog must contain projected species names"
    );
    assert!(
        html.contains("NONEXISTENT_STEM_0001"),
        "Catalog must contain projected bundle stems"
    );

    // 2. Performance SLA: 50 items rendered from SQLite state must execute in < 250ms (typically < 15ms)
    assert!(
        elapsed.as_millis() < 250,
        "Catalog rendering SLA violated: took {}ms (expected < 250ms)",
        elapsed.as_millis()
    );
}

#[test]
fn test_guardrail_source_code_view_rendering_never_imports_image_scanners() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let template_file = manifest_dir.join("src/catalog/template.rs");

    let content = fs::read_to_string(&template_file).expect("read catalog/template.rs");

    // Static source code guardrail:
    // Ensure catalog::template NEVER imports WalkDir, image::open, or filesystem scanners
    assert!(
        !content.contains("use walkdir::WalkDir;"),
        "catalog::template must never import WalkDir"
    );
    assert!(
        !content.contains("scan_input_tree"),
        "catalog::template must never call scan_input_tree"
    );
    assert!(
        !content.contains("image::open"),
        "catalog::template must never call image::open"
    );
}
