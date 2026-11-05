use avicull::organizer::*;
use avicull::types::*;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_format_inplace_filename_basic() {
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );

    // Compound sidecar extension
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            "CR2.xmp"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp"
    );

    // Extension with leading dot
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("20261025"),
            Some("Leh"),
            ".CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );
}

#[test]
fn test_format_inplace_filename_date_and_location_omission() {
    // Both omitted
    assert_eq!(
        format_inplace_filename("IMG_4821", "", "House Sparrow", None, None, "CR2"),
        "IMG_4821_House_Sparrow.CR2"
    );

    // Empty strings omitted
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some(""),
            Some("   "),
            "CR2"
        ),
        "IMG_4821_House_Sparrow.CR2"
    );

    // Unknown sentinels cleanly omitted
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("UnknownDate"),
            Some("UnknownLocation"),
            "jpg"
        ),
        "IMG_4821_House_Sparrow.jpg"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("Unknown"),
            Some("Unknown"),
            "jpg"
        ),
        "IMG_4821_House_Sparrow.jpg"
    );

    // Date formatted with hyphens normalized to YYYYMMDD
    assert_eq!(
        format_inplace_filename(
            "IMG_4821",
            "",
            "House Sparrow",
            Some("2026-10-25"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4821_House_Sparrow_20261025_Leh.CR2"
    );
}

#[test]
fn test_format_inplace_filename_routing_tiers() {
    // Needs Review
    assert_eq!(
        format_inplace_filename(
            "IMG_4822",
            "",
            "Needs_Review_Black_Redstart",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh.CR2"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4822",
            "",
            "_Needs_Review_Black_Redstart",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh.CR2"
    );

    // No Bird Detected
    assert_eq!(
        format_inplace_filename(
            "IMG_4823",
            "",
            "No Bird Detected",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4823_No_Bird_Detected_20261025_Leh.CR2"
    );
    assert_eq!(
        format_inplace_filename(
            "IMG_4823",
            "",
            "_No_Bird_Detected",
            Some("20261025"),
            Some("Leh"),
            "CR2"
        ),
        "IMG_4823_No_Bird_Detected_20261025_Leh.CR2"
    );
}

#[test]
fn test_is_already_inplace_named() {
    assert!(is_already_inplace_named(
        "IMG_4821_House_Sparrow_20261025_Leh",
        "House Sparrow"
    ));
    assert!(is_already_inplace_named(
        "IMG_4821_House_Sparrow",
        "House Sparrow"
    ));
    assert!(is_already_inplace_named(
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh",
        "Black Redstart"
    ));
    assert!(is_already_inplace_named(
        "IMG_4822_Needs_Review_Black_Redstart_20261025_Leh",
        "Needs_Review_Black_Redstart"
    ));
    assert!(is_already_inplace_named(
        "IMG_4823_No_Bird_Detected_20261025_Leh",
        "No Bird Detected"
    ));
    assert!(is_already_inplace_named(
        "IMG_4823_No_Bird_Detected_20261025_Leh",
        "_No_Bird_Detected"
    ));

    assert!(!is_already_inplace_named("IMG_4821", "House Sparrow"));
    assert!(!is_already_inplace_named("DSCRuff01", "Ruff"));
    assert!(!is_already_inplace_named(
        "IMG_4821_House_Sparrow_20261025_Leh",
        "Tibetan Snowcock"
    ));
}

#[test]
fn test_execute_inplace_bundle_rename_lockstep_all_files() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let primary = root.join("IMG_4821.CR2");
    let companion = root.join("IMG_4821.JPG");
    let sidecar1 = root.join("IMG_4821.xmp");
    let sidecar2 = root.join("IMG_4821.CR2.xmp");

    File::create(&primary)
        .unwrap()
        .write_all(b"raw-data")
        .unwrap();
    File::create(&companion)
        .unwrap()
        .write_all(b"jpeg-data")
        .unwrap();
    File::create(&sidecar1)
        .unwrap()
        .write_all(b"xmp-1")
        .unwrap();
    File::create(&sidecar2)
        .unwrap()
        .write_all(b"xmp-2")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle1".to_string(),
        source_inode: 12345,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![companion.clone()],
        sidecar_files: vec![sidecar1.clone(), sidecar2.clone()],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Rename must succeed");

    assert_eq!(renames.len(), 4);

    let expected_primary = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    let expected_companion = root.join("IMG_4821_House_Sparrow_20261025_Leh.JPG");
    let expected_sidecar1 = root.join("IMG_4821_House_Sparrow_20261025_Leh.xmp");
    let expected_sidecar2 = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2.xmp");

    assert!(expected_primary.exists(), "Primary must be renamed");
    assert!(expected_companion.exists(), "Companion must be renamed");
    assert!(expected_sidecar1.exists(), "Sidecar 1 must be renamed");
    assert!(expected_sidecar2.exists(), "Sidecar 2 must be renamed");

    assert!(!primary.exists(), "Old primary must not exist");
    assert!(!companion.exists(), "Old companion must not exist");
    assert!(!sidecar1.exists(), "Old sidecar 1 must not exist");
    assert!(!sidecar2.exists(), "Old sidecar 2 must not exist");

    assert_eq!(fs::read(&expected_primary).unwrap(), b"raw-data");
    assert_eq!(fs::read(&expected_companion).unwrap(), b"jpeg-data");
    assert_eq!(fs::read(&expected_sidecar1).unwrap(), b"xmp-1");
    assert_eq!(fs::read(&expected_sidecar2).unwrap(), b"xmp-2");
}

#[test]
fn test_execute_inplace_bundle_rename_idempotency_skip() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let primary = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    File::create(&primary)
        .unwrap()
        .write_all(b"raw-data")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle1".to_string(),
        source_inode: 12345,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821_House_Sparrow_20261025_Leh".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Should succeed without error");

    assert!(renames.is_empty(), "Idempotent rename must skip");
    assert!(primary.exists(), "Original file must remain untouched");
}

#[test]
fn test_execute_inplace_bundle_rename_collision_disambiguation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let existing_target = root.join("IMG_4821_House_Sparrow_20261025_Leh.CR2");
    File::create(&existing_target)
        .unwrap()
        .write_all(b"existing-target")
        .unwrap();

    let primary = root.join("IMG_4821.CR2");
    File::create(&primary)
        .unwrap()
        .write_all(b"new-raw-data")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "bundle2".to_string(),
        source_inode: 54321,
        trip_folder_name: "Leh".to_string(),
        original_stem: "IMG_4821".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let renames =
        execute_inplace_bundle_rename(&bundle, "House Sparrow", Some("20261025"), Some("Leh"))
            .expect("Collision disambiguation must succeed");

    assert_eq!(renames.len(), 1);
    let disambiguated_target = root.join("IMG_4821_House_Sparrow_20261025_Leh_1.CR2");
    assert!(disambiguated_target.exists());
    assert_eq!(fs::read(&disambiguated_target).unwrap(), b"new-raw-data");
    assert_eq!(fs::read(&existing_target).unwrap(), b"existing-target");
}

#[test]
fn test_inplace_mode_skips_rename_when_no_bird_detected() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let primary = root.join("IMG_9999.CR2");
    let companion = root.join("IMG_9999.JPG");
    let sidecar = root.join("IMG_9999.xmp");

    File::create(&primary)
        .unwrap()
        .write_all(b"landscape-raw")
        .unwrap();
    File::create(&companion)
        .unwrap()
        .write_all(b"landscape-jpeg")
        .unwrap();
    File::create(&sidecar)
        .unwrap()
        .write_all(b"landscape-xmp")
        .unwrap();

    let bundle = AssetBundle {
        bundle_id: "nobird1".to_string(),
        source_inode: 99999,
        trip_folder_name: "Bangalore".to_string(),
        original_stem: "IMG_9999".to_string(),
        primary_image: primary.clone(),
        companion_images: vec![companion.clone()],
        sidecar_files: vec![sidecar.clone()],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: Some("Bangalore".to_string()),
        relative_path: None,
    };

    let decision = BundleDecision {
        bundle_id: "nobird1".to_string(),
        burst_id: None,
        detection_confidence: 0.05,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.05,
        final_confidence: 0.05,
        assigned_species_slug: "Unknown_Bird".to_string(),
        routing_tier: RoutingTier::NoBirdDetected,
        is_user_override: false,
    };

    // When routing tier is NoBirdDetected, in-place renames must be empty (files untouched)
    let renames: Vec<(std::path::PathBuf, std::path::PathBuf)> = match decision.routing_tier {
        RoutingTier::NoBirdDetected => Vec::new(),
        RoutingTier::ConfirmedSpecies | RoutingTier::NeedsReview => {
            execute_inplace_bundle_rename(&bundle, "House Sparrow", None, None).unwrap()
        }
    };

    assert!(
        renames.is_empty(),
        "No-bird-detected bundles must not produce any rename operations"
    );
    assert!(
        primary.exists(),
        "Original primary image must remain untouched on disk"
    );
    assert!(
        companion.exists(),
        "Original companion image must remain untouched on disk"
    );
    assert!(
        sidecar.exists(),
        "Original sidecar file must remain untouched on disk"
    );
    assert!(
        !root.join("IMG_9999_No_Bird_Detected.CR2").exists(),
        "Must NOT rename file to No_Bird_Detected"
    );
}

#[test]
fn test_format_inplace_filename_two_level_nomenclature() {
    // 1. Bird format: <stem>_<Category>_<Subject>[_<YYYYMMDD>][_<Location>].<ext>
    let bird_name = format_inplace_filename(
        "DSC_0042",
        "Bird",
        "white-throated_kingfisher",
        Some("20261102"),
        Some("Western_Ghats"),
        "CR2",
    );
    assert_eq!(
        bird_name,
        "DSC_0042_Bird_white-throated_kingfisher_20261102_Western_Ghats.CR2"
    );

    // 2. Mammalia format
    let mammal_name = format_inplace_filename(
        "DSC_0105",
        "Mammalia",
        "malabar_giant_squirrel",
        Some("20261102"),
        Some("Western_Ghats"),
        "CR2",
    );
    assert_eq!(
        mammal_name,
        "DSC_0105_Mammalia_malabar_giant_squirrel_20261102_Western_Ghats.CR2"
    );

    // 3. Unclassified / Empty shot format: DSC_0300_No_Subject_Detected_20261102.CR2
    let unclassified_name = format_inplace_filename(
        "DSC_0300",
        "",
        "No_Subject_Detected",
        Some("20261102"),
        None,
        "CR2",
    );
    assert_eq!(
        unclassified_name,
        "DSC_0300_No_Subject_Detected_20261102.CR2"
    );

    // 4. Needs Review format: DSC_0042_Needs_Review_Bird_white-throated_kingfisher_20261102.CR2
    let needs_review_name = format_inplace_filename(
        "DSC_0042",
        "Bird",
        "Needs_Review_white-throated_kingfisher",
        Some("20261102"),
        None,
        "CR2",
    );
    assert_eq!(
        needs_review_name,
        "DSC_0042_Needs_Review_Bird_white-throated_kingfisher_20261102.CR2"
    );
}

#[test]
fn test_round_trip_parse_inplace_filename_two_level() {
    // Bird round-trip
    let bird_file = format_inplace_filename(
        "DSC_0042",
        "Bird",
        "white-throated_kingfisher",
        Some("20261102"),
        Some("Western_Ghats"),
        "CR2",
    );
    let parsed = parse_inplace_filename(&bird_file, "DSC_0042").expect("should parse bird file");
    assert_eq!(parsed.0, AssetCategory::Bird);
    assert_eq!(parsed.1, "white-throated_kingfisher");
    assert_eq!(parsed.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed.3.as_deref(), Some("20261102"));
    assert_eq!(parsed.4.as_deref(), Some("Western_Ghats"));

    // Mammalia round-trip
    let mammal_file = format_inplace_filename(
        "DSC_0105",
        "Mammalia",
        "malabar_giant_squirrel",
        Some("20261102"),
        Some("Western_Ghats"),
        "CR2",
    );
    let parsed_mammal =
        parse_inplace_filename(&mammal_file, "DSC_0105").expect("should parse mammal file");
    assert_eq!(parsed_mammal.0, AssetCategory::Mammalia);
    assert_eq!(parsed_mammal.1, "malabar_giant_squirrel");
    assert_eq!(parsed_mammal.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_mammal.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_mammal.4.as_deref(), Some("Western_Ghats"));

    // Insecta round-trip
    let insect_file = format_inplace_filename(
        "DSC_0088",
        "Insecta",
        "malabar_banded_peacock",
        Some("20261102"),
        Some("Western_Ghats"),
        "CR2",
    );
    let parsed_insect =
        parse_inplace_filename(&insect_file, "DSC_0088").expect("should parse insect file");
    assert_eq!(parsed_insect.0, AssetCategory::Insecta);
    assert_eq!(parsed_insect.1, "malabar_banded_peacock");
    assert_eq!(parsed_insect.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_insect.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_insect.4.as_deref(), Some("Western_Ghats"));

    // Scenery round-trip
    let scenery_file = format_inplace_filename(
        "DSC_0201",
        "Scenery",
        "aurora_borealis",
        Some("20261102"),
        Some("Tromso"),
        "CR2",
    );
    let parsed_scenery =
        parse_inplace_filename(&scenery_file, "DSC_0201").expect("should parse scenery file");
    assert_eq!(parsed_scenery.0, AssetCategory::Scenery);
    assert_eq!(parsed_scenery.1, "aurora_borealis");
    assert_eq!(parsed_scenery.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_scenery.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_scenery.4.as_deref(), Some("Tromso"));

    // Portrait (Other category) round-trip
    let portrait_file = format_inplace_filename(
        "IMG_5012",
        "Portrait",
        "headshot",
        Some("20261102"),
        Some("Studio"),
        "CR2",
    );
    let parsed_portrait =
        parse_inplace_filename(&portrait_file, "IMG_5012").expect("should parse portrait file");
    assert_eq!(
        parsed_portrait.0,
        AssetCategory::Other("Portrait".to_string())
    );
    assert_eq!(parsed_portrait.1, "headshot");
    assert_eq!(parsed_portrait.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_portrait.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_portrait.4.as_deref(), Some("Studio"));

    // Abstract (Other category) round-trip
    let abstract_file = format_inplace_filename(
        "IMG_5020",
        "Abstract",
        "glass_refraction",
        Some("20261102"),
        None,
        "JPG",
    );
    let parsed_abstract =
        parse_inplace_filename(&abstract_file, "IMG_5020").expect("should parse abstract file");
    assert_eq!(
        parsed_abstract.0,
        AssetCategory::Other("Abstract".to_string())
    );
    assert_eq!(parsed_abstract.1, "glass_refraction");
    assert_eq!(parsed_abstract.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_abstract.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_abstract.4, None);

    // Unclassified round-trip
    let unclassified_file = format_inplace_filename(
        "DSC_0300",
        "",
        "No_Subject_Detected",
        Some("20261102"),
        None,
        "CR2",
    );
    let parsed_unclassified = parse_inplace_filename(&unclassified_file, "DSC_0300")
        .expect("should parse unclassified file");
    assert_eq!(parsed_unclassified.0, AssetCategory::Bird);
    assert_eq!(parsed_unclassified.1, "No_Subject_Detected");
    assert_eq!(parsed_unclassified.2, RoutingTier::NoBirdDetected);
    assert_eq!(parsed_unclassified.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_unclassified.4, None);
}

#[test]
fn test_parse_inplace_filename_legacy_v1_promotion() {
    // Legacy v1 single-level bird file: DSC_0042_Painted_Stork.CR2
    // Must be recognized as legacy bird entry and promoted to AssetCategory::Bird,
    // WITHOUT splintering "Painted" as category and "Stork" as subject!
    let legacy_v1 = "DSC_0042_Painted_Stork.CR2";
    let parsed =
        parse_inplace_filename(legacy_v1, "DSC_0042").expect("should parse legacy v1 file");
    assert_eq!(
        parsed.0,
        AssetCategory::Bird,
        "Must promote to Bird category"
    );
    assert_eq!(
        parsed.1, "Painted_Stork",
        "Must not splinter Painted and Stork"
    );
    assert_eq!(parsed.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed.3, None);
    assert_eq!(parsed.4, None);

    // Legacy v1 with date and location
    let legacy_v1_with_meta = "DSC_0042_Painted_Stork_20261102_Western_Ghats.CR2";
    let parsed_meta = parse_inplace_filename(legacy_v1_with_meta, "DSC_0042")
        .expect("should parse legacy v1 with metadata");
    assert_eq!(parsed_meta.0, AssetCategory::Bird);
    assert_eq!(parsed_meta.1, "Painted_Stork");
    assert_eq!(parsed_meta.2, RoutingTier::ConfirmedSpecies);
    assert_eq!(parsed_meta.3.as_deref(), Some("20261102"));
    assert_eq!(parsed_meta.4.as_deref(), Some("Western_Ghats"));

    // Legacy v1 with Needs_Review
    let legacy_nr = "DSC_0042_Needs_Review_Painted_Stork_20261102.CR2";
    let parsed_nr =
        parse_inplace_filename(legacy_nr, "DSC_0042").expect("should parse legacy needs review");
    assert_eq!(parsed_nr.0, AssetCategory::Bird);
    assert_eq!(parsed_nr.1, "Painted_Stork");
    assert_eq!(parsed_nr.2, RoutingTier::NeedsReview);
    assert_eq!(parsed_nr.3.as_deref(), Some("20261102"));

    // Legacy v1 No_Bird_Detected
    let legacy_nobird = "DSC_0300_No_Bird_Detected_20261102.CR2";
    let parsed_nobird = parse_inplace_filename(legacy_nobird, "DSC_0300")
        .expect("should parse legacy no bird detected");
    assert_eq!(parsed_nobird.0, AssetCategory::Bird);
    assert_eq!(parsed_nobird.1, "No_Bird_Detected");
    assert_eq!(parsed_nobird.2, RoutingTier::NoBirdDetected);
    assert_eq!(parsed_nobird.3.as_deref(), Some("20261102"));
}

#[test]
fn test_infer_prediction_from_stem_all_categories() {
    // 1. Mammalia
    let mammal_stem = "1X7A9001_Mammalia_Malabar_Giant_Squirrel_20251002_Western_Ghats";
    let (cat, pred, tier) =
        infer_prediction_from_stem(mammal_stem).expect("should infer prediction from mammal stem");
    assert_eq!(cat, AssetCategory::Mammalia);
    assert_eq!(pred.folder_slug, "Mammalia_Malabar_Giant_Squirrel");
    assert_eq!(pred.common_name, "Malabar Giant Squirrel");
    assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    assert!(pred.combined_score >= 0.90);

    // 2. Insecta
    let insect_stem = "1X7A9002_Insecta_Peacock_Butterfly_20251002_Western_Ghats";
    let (cat, pred, tier) =
        infer_prediction_from_stem(insect_stem).expect("should infer prediction from insect stem");
    assert_eq!(cat, AssetCategory::Insecta);
    assert_eq!(pred.folder_slug, "Insecta_Peacock_Butterfly");
    assert_eq!(pred.common_name, "Peacock Butterfly");
    assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    assert!(pred.combined_score >= 0.90);

    // 3. Scenery
    let scenery_stem = "1X7A9003_Scenery_Pangong_Tso_20250915_Ladakh";
    let (cat, pred, tier) = infer_prediction_from_stem(scenery_stem)
        .expect("should infer prediction from scenery stem");
    assert_eq!(cat, AssetCategory::Scenery);
    assert_eq!(pred.folder_slug, "Scenery_Pangong_Tso");
    assert_eq!(pred.common_name, "Pangong Tso");
    assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    assert!(pred.combined_score >= 0.90);

    // 4. Needs Review Bird
    let nr_stem = "1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore";
    let (cat, pred, tier) = infer_prediction_from_stem(nr_stem)
        .expect("should infer prediction from needs review stem");
    assert_eq!(cat, AssetCategory::Bird);
    assert_eq!(pred.folder_slug, "Red-vented_Bulbul");
    assert_eq!(tier, RoutingTier::NeedsReview);
    assert!((pred.combined_score - 0.60).abs() < 0.05);

    // 5. Confirmed Bird
    let bird_stem = "1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi";
    let (cat, pred, tier) = infer_prediction_from_stem(bird_stem)
        .expect("should infer prediction from confirmed bird stem");
    assert_eq!(cat, AssetCategory::Bird);
    assert_eq!(pred.folder_slug, "Rose-ringed_Parakeet");
    assert_eq!(tier, RoutingTier::ConfirmedSpecies);
    assert!(pred.combined_score >= 0.90);

    // 6. Plain uninformative stem should return None
    assert!(infer_prediction_from_stem("IMG_4821").is_none());
    assert!(infer_prediction_from_stem("1X7A9001").is_none());
}

#[test]
fn test_infer_prediction_from_stem_rejects_numeric_suffixes() {
    // Camera shot files with numeric suffixes or duplicate counter tokens
    // should not have those numbers interpreted as species names.
    assert!(infer_prediction_from_stem("IMG_3172_01").is_none());
    assert!(infer_prediction_from_stem("IMG_3250_02").is_none());
    assert!(infer_prediction_from_stem("DSC_0042_1").is_none());
    assert!(infer_prediction_from_stem("IMG_1234_999").is_none());
    assert!(infer_prediction_from_stem("1X7A9001_02").is_none());
}
