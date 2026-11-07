use avicull::db::*;
use avicull::types::*;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_db_lifecycle_record_and_query() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");

    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let run_id = "run-001";
    db.record_run(
        run_id,
        Path::new("/photos/Trips"),
        Path::new("/photos/By_Species"),
        false,
        "{}",
    )
    .expect("record_run failed");

    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 999,
        trip_folder_name: "2025-11_Bharatpur".to_string(),
        original_stem: "DSC_4011".to_string(),
        primary_image: PathBuf::from("/photos/Trips/DSC_4011.JPG"),
        companion_images: vec![],
        sidecar_files: vec![PathBuf::from("/photos/Trips/DSC_4011.xmp")],
        capture_timestamp_ms: Some(1730000000000),
        gps_coords: Some(GpsCoordinate {
            latitude: 27.17,
            longitude: 77.52,
            source: LocationSource::ExifGps,
        }),
        resolved_location_name: None,
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: Some("burst-001".to_string()),
        detection_confidence: 0.95,
        bbox_xyxy_norm: Some([0.1, 0.1, 0.9, 0.9]),
        top_predictions: vec![SpeciesPrediction {
            taxon_id: "paisto1".to_string(),
            common_name: "Painted Stork".to_string(),
            folder_slug: "Painted_Stork".to_string(),
            scientific_name: "Mycteria leucocephala".to_string(),
            visual_prob: 0.95,
            geo_prior: 0.98,
            combined_score: 0.96,
        }],
        frame_confidence: 0.96,
        final_confidence: 0.96,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    db.record_bundles(
        run_id,
        std::slice::from_ref(&decision),
        std::slice::from_ref(&bundle),
    )
    .expect("record_bundles failed");

    let op = HardlinkOperation {
        op_id: None,
        run_id: run_id.to_string(),
        bundle_id: "b1".to_string(),
        source_path: PathBuf::from("/photos/Trips/DSC_4011.JPG"),
        dest_path: PathBuf::from("/photos/By_Species/Painted_Stork/DSC_4011_0.96.JPG"),
        source_inode: 999,
        status: "linked".to_string(),
        operation_type: "hardlink".to_string(),
        executed_at: Some("2026-10-24T12:00:00Z".to_string()),
    };

    db.record_hardlink_operations(std::slice::from_ref(&op))
        .expect("record_hardlink_operations failed");

    // Query operations back
    let ops = db
        .get_run_operations(run_id)
        .expect("get_run_operations failed");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].dest_path, op.dest_path);
    assert_eq!(ops[0].status, "linked");

    // Query bundles for relink
    let loaded = db
        .get_all_bundles(Some(run_id))
        .expect("get_all_bundles failed");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0.assigned_species_slug, "Painted_Stork");
    assert_eq!(loaded[0].1.original_stem, "DSC_4011");
}

#[test]
fn test_update_bundle_routing_tier() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let run_id = "run-002";
    db.record_run(
        run_id,
        Path::new("/photos/Trips"),
        Path::new("/photos/By_Species"),
        false,
        "{}",
    )
    .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle_xyz".to_string(),
        source_inode: 1234,
        trip_folder_name: "Trip".to_string(),
        original_stem: "IMG_001".to_string(),
        primary_image: PathBuf::from("/photos/Trips/IMG_001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "bundle_xyz".to_string(),
        burst_id: None,
        detection_confidence: 0.85,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.60,
        final_confidence: 0.60,
        assigned_species_slug: "Common_Merganser".to_string(),
        routing_tier: RoutingTier::NeedsReview, // Initially NeedsReview
        is_user_override: false,
    };

    db.record_bundles(run_id, &[decision], &[bundle]).unwrap();

    // Verify initial routing tier
    let initial = db.get_all_bundles(Some(run_id)).unwrap();
    assert_eq!(initial[0].0.routing_tier, RoutingTier::NeedsReview);

    // Update to ConfirmedSpecies (e.g. after relink with lower threshold)
    db.update_bundle_routing_tier("bundle_xyz", RoutingTier::ConfirmedSpecies)
        .unwrap();

    let updated = db.get_all_bundles(Some(run_id)).unwrap();
    assert_eq!(updated[0].0.routing_tier, RoutingTier::ConfirmedSpecies);
}

fn dummy_bundle() -> AssetBundle {
    AssetBundle {
        bundle_id: "b_test_1".to_string(),
        source_inode: 12345,
        trip_folder_name: "TripA".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: PathBuf::from("/input/TripA/DSC_0001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: Some(1730000000000),
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    }
}

fn dummy_decision() -> BundleDecision {
    BundleDecision {
        bundle_id: "b_test_1".to_string(),
        burst_id: None,
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "Passer_domesticus".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    }
}

#[test]
fn test_manifest_db_portable_bundle_lookup_and_override() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    let mut bundle = dummy_bundle();
    bundle.relative_path = Some(PathBuf::from("TripA/DSC_0001.JPG"));
    let mut decision = dummy_decision();
    decision.is_user_override = true;

    db.record_run("run_1", "/input", "/output", false, "{}")
        .unwrap();
    db.record_bundle("run_1", &bundle, &decision).unwrap();

    let map = db.get_existing_bundle_map().unwrap();
    // Inode changed (e.g. simulated different machine)
    bundle.source_inode = 999999;
    let found = map.get_by_bundle(&bundle);
    assert!(found.is_some());
    assert!(found.unwrap().0.is_user_override);
    assert!(map.get_by_portable_key("TripA", "DSC_0001").is_some());
}

#[test]
fn test_poisoned_mutex_graceful_error() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    // Deliberately poison the inner connection mutex on a background thread.
    let raw = std::sync::Arc::clone(db.raw_conn());
    let _ = std::thread::spawn(move || {
        let _guard = raw.lock().unwrap();
        panic!("deliberate panic while holding mutex to test poison handling");
    })
    .join();

    // Calling any ManifestDb method must return an error rather than panicking.
    let result = db.get_latest_run_id();
    assert!(
        result.is_err(),
        "Expected Err on poisoned mutex, but got Ok"
    );
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("Database connection mutex poisoned"),
        "Error message '{err_msg}' does not mention mutex poisoning"
    );

    // Verify error matches AvicullError::DatabaseError variant
    let avicull_err = err
        .downcast_ref::<AvicullError>()
        .expect("Expected error to downcast to AvicullError");
    match avicull_err {
        AvicullError::DatabaseError(msg) => {
            assert!(msg.contains("Database connection mutex poisoned"));
        }
        other => panic!("Expected AvicullError::DatabaseError, got {other:?}"),
    }
}

#[test]
fn test_schema_migration_idempotent() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let raw = std::sync::Arc::clone(db.raw_conn());
    let conn = raw.lock().expect("lock conn");

    // Running migration multiple times must be idempotent
    migrate_bundle_decisions(&conn).expect("first migration failed");
    migrate_bundle_decisions(&conn).expect("second migration failed");

    // Verify columns exist on bundle_decisions
    let mut stmt = conn
        .prepare("PRAGMA table_info(bundle_decisions)")
        .expect("prepare pragma");
    let mut rows = stmt.query([]).expect("query pragma");
    let mut has_category = false;
    let mut has_embedding = false;
    while let Some(row) = rows.next().expect("next row") {
        let name: String = row.get(1).expect("col name");
        if name == "category" {
            has_category = true;
        } else if name == "visual_embedding" {
            has_embedding = true;
        }
    }
    assert!(has_category, "bundle_decisions missing category column");
    assert!(
        has_embedding,
        "bundle_decisions missing visual_embedding column"
    );
}

#[test]
fn test_insert_and_query_with_512_dim_embedding() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open_or_create failed");

    let raw = std::sync::Arc::clone(db.raw_conn());
    let conn = raw.lock().expect("lock conn");

    let mut decision = dummy_decision();
    decision.bundle_id = "mammal_001".to_string();
    decision.assigned_species_slug = "Malabar_Giant_Squirrel".to_string();

    let embedding: Vec<f32> = (0..512).map(|i| (i as f32) / 512.0).collect();

    insert_bundle_decision(&conn, &decision, &AssetCategory::Mammalia, Some(&embedding))
        .expect("insert_bundle_decision failed");

    let results = query_decisions(&conn).expect("query_decisions failed");
    assert_eq!(results.len(), 1);

    let (dec, cat, emb) = &results[0];
    assert_eq!(dec.bundle_id, "mammal_001");
    assert_eq!(dec.assigned_species_slug, "Malabar_Giant_Squirrel");
    assert_eq!(*cat, AssetCategory::Mammalia);

    let queried_emb = emb.as_ref().expect("expected embedding");
    assert_eq!(queried_emb.len(), 512);
    for (i, val) in queried_emb.iter().enumerate() {
        assert!(
            (val - (i as f32 / 512.0)).abs() < 1e-6,
            "embedding mismatch at index {i}"
        );
    }
}

#[test]
fn test_legacy_schema_migration_without_data_loss() {
    let dir = tempdir().expect("create temp dir");
    let db_path = dir.path().join("legacy.sqlite");

    // Manually create legacy table without category and visual_embedding columns
    let conn = rusqlite::Connection::open(&db_path).expect("open legacy db");
    conn.execute(
        "CREATE TABLE bundle_decisions (
            bundle_id TEXT PRIMARY KEY,
            burst_id TEXT,
            detection_conf REAL NOT NULL,
            bbox_json TEXT,
            assigned_species_slug TEXT NOT NULL,
            frame_conf REAL NOT NULL,
            final_conf REAL NOT NULL,
            routing_tier TEXT NOT NULL,
            top5_predictions_json TEXT NOT NULL,
            is_user_override INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )
    .expect("create legacy table");

    // Insert legacy row
    conn.execute(
        "INSERT INTO bundle_decisions (
            bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug,
            frame_conf, final_conf, routing_tier, top5_predictions_json, is_user_override
        ) VALUES (
            'legacy_001', 'burst_1', 0.95, '[0.1, 0.1, 0.9, 0.9]', 'Painted_Stork',
            0.95, 0.95, 'ConfirmedSpecies', '[]', 0
        )",
        [],
    )
    .expect("insert legacy row");

    // Run migration
    migrate_bundle_decisions(&conn).expect("migration failed");

    // Query decisions to verify data preserved with defaults
    let results = query_decisions(&conn).expect("query_decisions failed");
    assert_eq!(results.len(), 1);
    let (dec, cat, emb) = &results[0];
    assert_eq!(dec.bundle_id, "legacy_001");
    assert_eq!(dec.assigned_species_slug, "Painted_Stork");
    assert_eq!(
        *cat,
        AssetCategory::Bird,
        "expected default category 'Bird'"
    );
    assert!(emb.is_none(), "legacy row should have no embedding");
}

#[test]
fn test_embedding_serialization_roundtrip_and_errors() {
    let valid_vec: Vec<f32> = (0..512).map(|i| i as f32 * 0.1).collect();
    let bytes = serialize_embedding_512(&valid_vec).expect("serialize failed");
    assert_eq!(bytes.len(), 2048, "serialized bytes must be exactly 2048");

    let deserialized = deserialize_embedding_512(&bytes).expect("deserialize failed");
    assert_eq!(deserialized.len(), 512);
    for (a, b) in valid_vec.iter().zip(deserialized.iter()) {
        assert!((a - b).abs() < 1e-6);
    }

    // Invalid dimension
    let short_vec = vec![1.0f32; 511];
    assert!(
        serialize_embedding_512(&short_vec).is_err(),
        "511-dim must fail serialization"
    );

    let long_vec = vec![1.0f32; 513];
    assert!(
        serialize_embedding_512(&long_vec).is_err(),
        "513-dim must fail serialization"
    );

    // Invalid byte lengths
    let short_bytes = vec![0u8; 2047];
    assert!(
        deserialize_embedding_512(&short_bytes).is_err(),
        "2047 bytes must fail deserialization"
    );

    let long_bytes = vec![0u8; 2049];
    assert!(
        deserialize_embedding_512(&long_bytes).is_err(),
        "2049 bytes must fail deserialization"
    );
}

#[test]
fn test_legacy_schema_migration_and_query_decisions_reconciles_all_non_avian_categories() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");

    conn.execute(
        "CREATE TABLE bundle_decisions (
            bundle_id TEXT PRIMARY KEY,
            burst_id TEXT,
            detection_conf REAL NOT NULL,
            bbox_json TEXT,
            assigned_species_slug TEXT NOT NULL,
            frame_conf REAL NOT NULL,
            final_conf REAL NOT NULL,
            routing_tier TEXT NOT NULL,
            top5_predictions_json TEXT NOT NULL,
            is_user_override INTEGER NOT NULL DEFAULT 0,
            category TEXT NOT NULL DEFAULT 'Bird'
        )",
        [],
    )
    .expect("create legacy table");

    // Seed rows with legacy erroneous categories
    let sample_cases = [
        ("bd_marmot", "Himalayan_Marmot", "Bird", "ConfirmedSpecies"),
        (
            "bd_portrait",
            "Close-up_Human_Portrait",
            "Bird",
            "ConfirmedSpecies",
        ),
        ("bd_dunes", "Desert_Dunes", "Bird", "ConfirmedSpecies"),
        ("bd_banyan", "Banyan_Tree", "Botanical", "ConfirmedSpecies"),
        ("bd_stork", "Painted_Stork", "Bird", "ConfirmedSpecies"),
        (
            "bd_wheatear",
            "Desert_Wheatear",
            "Scenery",
            "ConfirmedSpecies",
        ),
        (
            "bd_vulture",
            "Bearded_Vulture",
            "Mammalia",
            "ConfirmedSpecies",
        ),
    ];

    for (bid, slug, cat, tier) in sample_cases {
        conn.execute(
            "INSERT INTO bundle_decisions (
                bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug,
                frame_conf, final_conf, routing_tier, top5_predictions_json, is_user_override, category
            ) VALUES (?1, NULL, 0.9, '[]', ?2, 0.9, 0.9, ?3, '[]', 0, ?4)",
            rusqlite::params![bid, slug, tier, cat],
        )
        .expect("insert test row");
    }

    // Run migration
    migrate_bundle_decisions(&conn).expect("migration failed");

    // Query decisions to verify categories and slugs are reconciled
    let results = query_decisions(&conn).expect("query_decisions failed");
    let lookup: std::collections::HashMap<String, (AssetCategory, String)> = results
        .into_iter()
        .map(|(dec, cat, _)| (dec.bundle_id, (cat, dec.assigned_species_slug)))
        .collect();

    // 1. Himalayan Marmot must be Mammalia, not Bird
    let (marmot_cat, _) = lookup.get("bd_marmot").expect("found marmot");
    assert_eq!(*marmot_cat, AssetCategory::Mammalia);

    // 2. Human Portrait must be Portrait, not Bird
    let (portrait_cat, _) = lookup.get("bd_portrait").expect("found portrait");
    assert_eq!(*portrait_cat, AssetCategory::Portrait);

    // 3. Desert Dunes must be Scenery, not Bird
    let (dunes_cat, _) = lookup.get("bd_dunes").expect("found dunes");
    assert_eq!(*dunes_cat, AssetCategory::Scenery);

    // 4. Banyan Tree must be NoSubjectDetected, slug _No_Subject_Detected
    let (banyan_cat, banyan_slug) = lookup.get("bd_banyan").expect("found banyan");
    assert_eq!(*banyan_cat, AssetCategory::NoSubjectDetected);
    assert_eq!(banyan_slug, "_No_Subject_Detected");

    // 5. Painted Stork must remain Bird
    let (stork_cat, _) = lookup.get("bd_stork").expect("found stork");
    assert_eq!(*stork_cat, AssetCategory::Bird);

    // 6. Desert Wheatear must be restored to Bird, not Scenery
    let (wheatear_cat, _) = lookup.get("bd_wheatear").expect("found wheatear");
    assert_eq!(*wheatear_cat, AssetCategory::Bird);

    // 7. Bearded Vulture must be restored to Bird, not Mammalia
    let (vulture_cat, _) = lookup.get("bd_vulture").expect("found vulture");
    assert_eq!(*vulture_cat, AssetCategory::Bird);
}
