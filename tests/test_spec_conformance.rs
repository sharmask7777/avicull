use avicull::db::*;
use avicull::organizer::*;
use avicull::scanner::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;

#[test]
fn test_nested_and_parent_coexistence_via_hardlinks() {
    let dir = tempdir().expect("create temp dir");
    let photos_root = dir.path().join("Photos");
    let leh_trip = photos_root.join("Leh");
    fs::create_dir_all(&leh_trip).unwrap();

    let photo1 = leh_trip.join("IMG_0001.JPG");
    File::create(&photo1)
        .unwrap()
        .write_all(b"SAMPLE LEH BIRD PHOTO")
        .unwrap();

    let src_meta = fs::metadata(&photo1).unwrap();
    let src_ino = src_meta.ino();
    assert_eq!(src_meta.nlink(), 1);

    // 1. Organize Leh trip into its own birds_hierarchy
    let leh_hierarchy = leh_trip.join("birds_hierarchy");
    validate_volume_and_disjointness(&leh_trip, &leh_hierarchy).unwrap();
    let leh_bundles = scan_input_tree_with_skip(&leh_trip, Some(&leh_hierarchy)).unwrap();
    assert_eq!(leh_bundles.len(), 1);

    let decision_leh = BundleDecision {
        bundle_id: leh_bundles[0].bundle_id.clone(),
        burst_id: Some("burst-01".to_string()),
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "Tibetan_Snowcock".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
    };

    let db_leh_path = leh_hierarchy.join(".avicull/manifest.sqlite");
    let db_leh = ManifestDb::open_or_create(&db_leh_path).unwrap();
    db_leh
        .record_run("run-leh-1", &leh_trip, &leh_hierarchy, false, "{}")
        .unwrap();
    db_leh
        .record_bundles("run-leh-1", &[decision_leh.clone()], &leh_bundles)
        .unwrap();
    let ops_leh = materialize_bundle_hardlinks(
        &leh_bundles[0],
        &decision_leh,
        &leh_hierarchy,
        "run-leh-1",
        false,
    )
    .unwrap();
    db_leh.record_hardlink_operations(&ops_leh).unwrap();

    // After Leh organize: link count is 2 (source + leh_hierarchy)
    let meta_after_leh = fs::metadata(&photo1).unwrap();
    assert_eq!(meta_after_leh.nlink(), 2);
    let linked_leh = leh_hierarchy.join("Tibetan_Snowcock/IMG_0001_0.95.JPG");
    assert!(linked_leh.exists());
    assert_eq!(fs::metadata(&linked_leh).unwrap().ino(), src_ino);

    // 2. Organize parent directory Photos into its own birds_hierarchy
    let parent_hierarchy = photos_root.join("birds_hierarchy");
    validate_volume_and_disjointness(&photos_root, &parent_hierarchy).unwrap();

    // Verify scan_input_tree_with_skip skips both leh_hierarchy AND parent_hierarchy!
    let parent_bundles = scan_input_tree_with_skip(&photos_root, Some(&parent_hierarchy)).unwrap();
    assert_eq!(
        parent_bundles.len(),
        1,
        "Scanning parent must skip nested birds_hierarchy directories"
    );

    let decision_parent = BundleDecision {
        bundle_id: parent_bundles[0].bundle_id.clone(),
        burst_id: Some("burst-01".to_string()),
        detection_confidence: 0.95,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.95,
        final_confidence: 0.95,
        assigned_species_slug: "Tibetan_Snowcock".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
    };

    let db_parent_path = parent_hierarchy.join(".avicull/manifest.sqlite");
    let db_parent = ManifestDb::open_or_create(&db_parent_path).unwrap();
    db_parent
        .record_run("run-parent-1", &photos_root, &parent_hierarchy, false, "{}")
        .unwrap();
    db_parent
        .record_bundles("run-parent-1", &[decision_parent.clone()], &parent_bundles)
        .unwrap();
    let ops_parent = materialize_bundle_hardlinks(
        &parent_bundles[0],
        &decision_parent,
        &parent_hierarchy,
        "run-parent-1",
        false,
    )
    .unwrap();
    db_parent.record_hardlink_operations(&ops_parent).unwrap();

    // After parent organize: link count is 3 (source + leh_hierarchy + parent_hierarchy)!
    let meta_after_parent = fs::metadata(&photo1).unwrap();
    assert_eq!(
        meta_after_parent.nlink(),
        3,
        "Photo must co-exist across both child and parent hierarchies via hard links"
    );
    let linked_parent = parent_hierarchy.join("Tibetan_Snowcock/IMG_0001_0.95.JPG");
    assert!(linked_parent.exists());
    assert_eq!(fs::metadata(&linked_parent).unwrap().ino(), src_ino);

    // Clean parent hierarchy: link count drops from 3 to 2, child hierarchy still exists
    clean_run_hardlinks(&db_parent, "run-parent-1").unwrap();
    assert!(!linked_parent.exists());
    assert!(linked_leh.exists());
    assert_eq!(fs::metadata(&photo1).unwrap().nlink(), 2);

    // Clean child hierarchy: link count drops from 2 to 1, source file completely pristine
    clean_run_hardlinks(&db_leh, "run-leh-1").unwrap();
    assert!(!linked_leh.exists());
    assert_eq!(fs::metadata(&photo1).unwrap().nlink(), 1);
}

#[test]
fn test_incremental_auto_detection_of_newer_files() {
    let dir = tempdir().expect("create temp dir");
    let input = dir.path().join("Trip");
    let output = input.join("birds_hierarchy");
    fs::create_dir_all(&input).unwrap();

    let f1 = input.join("IMG_1001.JPG");
    let f2 = input.join("IMG_1002.JPG");
    File::create(&f1).unwrap().write_all(b"IMG 1").unwrap();
    File::create(&f2).unwrap().write_all(b"IMG 2").unwrap();

    let db_path = output.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();

    // First scan & organize of initial 2 files
    let bundles_v1 = scan_input_tree_with_skip(&input, Some(&output)).unwrap();
    assert_eq!(bundles_v1.len(), 2);

    let decisions_v1: Vec<_> = bundles_v1
        .iter()
        .map(|b| BundleDecision {
            bundle_id: b.bundle_id.clone(),
            burst_id: Some("burst-01".to_string()),
            detection_confidence: 0.90,
            bbox_xyxy_norm: None,
            top_predictions: vec![],
            frame_confidence: 0.90,
            final_confidence: 0.90,
            assigned_species_slug: "Painted_Stork".to_string(),
            routing_tier: RoutingTier::ConfirmedSpecies,
        })
        .collect();

    db.record_run("run-1", &input, &output, false, "{}")
        .unwrap();
    db.record_bundles("run-1", &decisions_v1, &bundles_v1)
        .unwrap();
    for (b, d) in bundles_v1.iter().zip(decisions_v1.iter()) {
        let ops = materialize_bundle_hardlinks(b, d, &output, "run-1", false).unwrap();
        db.record_hardlink_operations(&ops).unwrap();
    }

    // Verify existing map has 2 bundles
    let cached_map = db.get_existing_bundle_map().unwrap();
    assert_eq!(cached_map.len(), 2);

    // Now add a 3rd newer file
    let f3 = input.join("IMG_1003.JPG");
    File::create(&f3).unwrap().write_all(b"IMG 3 NEW").unwrap();

    // Second scan
    let bundles_v2 = scan_input_tree_with_skip(&input, Some(&output)).unwrap();
    assert_eq!(bundles_v2.len(), 3);

    let new_bundles: Vec<_> = bundles_v2
        .iter()
        .filter(|b| !cached_map.contains_key(&b.bundle_id))
        .collect();
    assert_eq!(new_bundles.len(), 1);
    assert_eq!(new_bundles[0].original_stem, "IMG_1003");

    // Organize only the new bundle
    let new_decision = BundleDecision {
        bundle_id: new_bundles[0].bundle_id.clone(),
        burst_id: Some("burst-02".to_string()),
        detection_confidence: 0.92,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.92,
        final_confidence: 0.92,
        assigned_species_slug: "Painted_Stork".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
    };

    db.record_run("run-2", &input, &output, false, "{}")
        .unwrap();
    db.record_bundles(
        "run-2",
        &[new_decision.clone()],
        &[(*new_bundles[0]).clone()],
    )
    .unwrap();
    let ops2 = materialize_bundle_hardlinks(new_bundles[0], &new_decision, &output, "run-2", false)
        .unwrap();
    db.record_hardlink_operations(&ops2).unwrap();

    // Verify all 3 files now exist in output
    assert!(output.join("Painted_Stork/IMG_1001_0.90.JPG").exists());
    assert!(output.join("Painted_Stork/IMG_1002_0.90.JPG").exists());
    assert!(output.join("Painted_Stork/IMG_1003_0.92.JPG").exists());

    let final_cached = db.get_existing_bundle_map().unwrap();
    assert_eq!(final_cached.len(), 3);
}

#[test]
fn test_no_bird_detected_routes_to_special_tier_folder() {
    let dir = tempdir().expect("create temp dir");
    let input = dir.path().join("Trip");
    let output = input.join("birds_hierarchy");
    fs::create_dir_all(&input).unwrap();

    let f1 = input.join("IMG_EMPTY.JPG");
    File::create(&f1)
        .unwrap()
        .write_all(b"EMPTY LANDSCAPE")
        .unwrap();

    let bundles = scan_input_tree_with_skip(&input, Some(&output)).unwrap();
    assert_eq!(bundles.len(), 1);

    let decision = BundleDecision {
        bundle_id: bundles[0].bundle_id.clone(),
        burst_id: None,
        detection_confidence: 0.05, // Below 0.25 -> NoBirdDetected
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.05,
        final_confidence: 0.05,
        assigned_species_slug: "Unknown_Bird".to_string(),
        routing_tier: RoutingTier::NoBirdDetected,
    };

    let target_dir = compute_target_dir(&output, &decision);
    assert_eq!(target_dir, output.join("_No_Bird_Detected"));

    let ops =
        materialize_bundle_hardlinks(&bundles[0], &decision, &output, "run-empty", false).unwrap();
    assert_eq!(
        ops[0].dest_path,
        output.join("_No_Bird_Detected/IMG_EMPTY_0.05.JPG")
    );
    assert!(output.join("_No_Bird_Detected/IMG_EMPTY_0.05.JPG").exists());
}
