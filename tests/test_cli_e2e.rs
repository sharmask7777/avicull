use avicull::db::*;
use avicull::organizer::*;
use avicull::scanner::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;

#[test]
fn test_cli_e2e_full_lifecycle() {
    let dir = tempdir().expect("create temp dir");
    let input_dir = dir.path().join("Trips");
    let output_dir = dir.path().join("Birds_By_Species");

    // Trip 1
    let trip1 = input_dir.join("2025-11_Bharatpur_Keoladeo");
    fs::create_dir_all(&trip1).unwrap();
    let t1_img1 = trip1.join("DSC_4011.JPG");
    let t1_xmp1 = trip1.join("DSC_4011.xmp");
    let t1_raw = trip1.join("DSC_4015.ARW");
    File::create(&t1_img1)
        .unwrap()
        .write_all(b"JPEG 4011")
        .unwrap();
    File::create(&t1_xmp1)
        .unwrap()
        .write_all(b"XMP 4011")
        .unwrap();
    File::create(&t1_raw)
        .unwrap()
        .write_all(b"ARW 4015")
        .unwrap();

    // Trip 2
    let trip2 = input_dir.join("2026-01_Ranganathittu");
    fs::create_dir_all(&trip2).unwrap();
    let t2_img1 = trip2.join("DSC_4011.JPG"); // Same stem as trip 1, different file!
    let t2_xmp1 = trip2.join("DSC_4011.xmp");
    File::create(&t2_img1)
        .unwrap()
        .write_all(b"JPEG 4011 TRIP 2")
        .unwrap();
    File::create(&t2_xmp1)
        .unwrap()
        .write_all(b"XMP 4011 TRIP 2")
        .unwrap();

    let meta_t1 = fs::metadata(&t1_img1).unwrap();
    let meta_t2 = fs::metadata(&t2_img1).unwrap();
    assert_ne!(meta_t1.ino(), meta_t2.ino(), "Different inodes");

    // 1. Scan validation
    validate_volume_and_disjointness(&input_dir, &output_dir).expect("Preflight validation failed");
    let bundles = scan_input_tree(&input_dir).expect("Scan failed");
    assert_eq!(bundles.len(), 3, "Expected 3 bundles total");

    // 2. Mock decision generation
    let run_id = "run-test-e2e";
    let db_path = output_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).expect("open db failed");
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")
        .unwrap();

    let mut decisions = Vec::new();
    for bundle in &bundles {
        let (slug, conf) = if bundle.trip_folder_name.contains("Bharatpur") {
            ("Painted_Stork", 0.96)
        } else {
            ("Painted_Stork", 0.54) // Trip 2 has Painted Stork below 0.65 -> Needs Review
        };

        decisions.push(BundleDecision {
            bundle_id: bundle.bundle_id.clone(),
            burst_id: Some("burst-001".to_string()),
            detection_confidence: 0.95,
            bbox_xyxy_norm: None,
            top_predictions: vec![],
            frame_confidence: conf,
            final_confidence: conf,
            assigned_species_slug: slug.to_string(),
            routing_tier: if conf >= 0.65 {
                RoutingTier::ConfirmedSpecies
            } else {
                RoutingTier::NeedsReview
            },
        });
    }

    db.record_bundles(run_id, &decisions, &bundles).unwrap();

    // 3. Materialize hard links
    let mut all_ops = Vec::new();
    for (bundle, decision) in bundles.iter().zip(decisions.iter()) {
        let ops = materialize_bundle_hardlinks(bundle, decision, &output_dir, run_id, false)
            .expect("materialize failed");
        all_ops.extend(ops);
    }
    db.record_hardlink_operations(&all_ops).unwrap();

    // Verify Confirmed folder for Trip 1
    let linked_t1_img = output_dir.join("Painted_Stork/DSC_4011_0.96.JPG");
    let linked_t1_xmp = output_dir.join("Painted_Stork/DSC_4011_0.96.xmp");
    assert!(
        linked_t1_img.exists(),
        "Trip 1 image should be in Painted_Stork"
    );
    assert!(
        linked_t1_xmp.exists(),
        "Trip 1 xmp should be in Painted_Stork"
    );
    assert_eq!(fs::metadata(&linked_t1_img).unwrap().ino(), meta_t1.ino());
    assert_eq!(fs::metadata(&linked_t1_img).unwrap().nlink(), 2);

    // Verify Needs_Review folder for Trip 2
    let linked_t2_img = output_dir.join("_Needs_Review/Painted_Stork/DSC_4011_0.54.JPG");
    assert!(
        linked_t2_img.exists(),
        "Trip 2 image should be in _Needs_Review"
    );
    assert_eq!(fs::metadata(&linked_t2_img).unwrap().ino(), meta_t2.ino());
    assert_eq!(fs::metadata(&linked_t2_img).unwrap().nlink(), 2);

    // 4. Instant Relink:
    // Suppose on relink, Trip 2 is re-scored or assigned confidence 0.96.
    // Since Painted_Stork already has DSC_4011_0.96.JPG from Trip 1 (with different inode),
    // Trip 2 must be disambiguated with trip prefix: 2026-01_Ranganathittu__DSC_4011_0.96.JPG!
    let cached = db
        .get_all_bundles(Some(run_id))
        .expect("get_all_bundles failed");
    for (mut dec, b) in cached {
        if b.trip_folder_name.contains("Ranganathittu") {
            dec.final_confidence = 0.96;
            dec.routing_tier = RoutingTier::ConfirmedSpecies;
            materialize_bundle_hardlinks(&b, &dec, &output_dir, run_id, false).unwrap();
        }
    }

    let disambiguated_t2 =
        output_dir.join("Painted_Stork/2026-01_Ranganathittu__DSC_4011_0.96.JPG");
    assert!(
        disambiguated_t2.exists(),
        "Relinked colliding file must be disambiguated with trip prefix"
    );

    // 5. Clean
    clean_run_hardlinks(&db, run_id).expect("clean failed");
    assert!(!linked_t1_img.exists(), "Output files must be removed");
    assert_eq!(
        fs::metadata(&t1_img1).unwrap().nlink(),
        1,
        "Input nlink must be restored to 1"
    );
}
