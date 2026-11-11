use avicull::db::ManifestDb;
use avicull::organizer::apply_bundle_correction_with_category;
use avicull::types::{
    AssetBundle, AssetCategory, BundleDecision, Classification, OrganizationMode, RoutingTier,
};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_correct_non_avian_granular_and_top_level_inplace() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let input_dir = dir.path().join("input");
    fs::create_dir_all(&input_dir)?;
    let img_path = input_dir.join("DSC_0001.JPG");
    fs::write(&img_path, b"fake jpeg payload")?;

    let db_path = input_dir.join(".avicull").join("manifest.sqlite");
    fs::create_dir_all(db_path.parent().unwrap())?;
    let db = ManifestDb::open_or_create(&db_path)?;

    let run_id = "run_test";
    db.record_run(
        run_id,
        &input_dir,
        &input_dir.join("birds_hierarchy"),
        false,
        "{}",
    )?;

    let src_meta = fs::metadata(&img_path)?;
    let src_ino = src_meta.ino();

    let bundle = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: src_ino,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: img_path.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0001.JPG")),
    };
    let dec = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Bird, "House_Sparrow"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };
    db.record_bundles(run_id, &[dec], &[bundle])?;

    // 1. InPlace mode: override to granular mammal
    let res = apply_bundle_correction_with_category(
        &db,
        &input_dir,
        OrganizationMode::InPlace,
        "DSC_0001",
        "Mammalia",
        "Snow_Leopard",
        Some(&input_dir),
        false,
    )?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.category(), &AssetCategory::Mammalia);
    assert_eq!(res[0].0.entity_slug(), Some("Snow_Leopard"));
    assert_eq!(res[0].0.assigned_species_slug(), "Snow_Leopard");
    assert!(res[0].0.is_user_override);

    // Verify source file on disk is strictly untouched
    assert!(img_path.exists());

    // 2. InPlace mode: override to top-level Scenery with blank species
    let res_top = apply_bundle_correction_with_category(
        &db,
        &input_dir,
        OrganizationMode::InPlace,
        "DSC_0001",
        "Scenery",
        "",
        Some(&input_dir),
        false,
    )?;
    assert_eq!(res_top.len(), 1);
    assert_eq!(res_top[0].0.category(), &AssetCategory::Scenery);
    assert_eq!(res_top[0].0.entity_slug(), None);
    assert_eq!(res_top[0].0.assigned_species_slug(), "");
    assert!(res_top[0].0.is_user_override);
    assert!(img_path.exists());

    // 3. Custom bird species that is not in Clements taxonomy (e.g. hybrid)
    let res_custom_bird = apply_bundle_correction_with_category(
        &db,
        &input_dir,
        OrganizationMode::InPlace,
        "DSC_0001",
        "Bird",
        "Gyrfalcon_x_Peregrine",
        Some(&input_dir),
        false,
    )?;
    assert_eq!(res_custom_bird.len(), 1);
    assert_eq!(res_custom_bird[0].0.category(), &AssetCategory::Bird);
    assert_eq!(
        res_custom_bird[0].0.entity_slug(),
        Some("Gyrfalcon_x_Peregrine")
    );
    assert!(res_custom_bird[0].0.is_user_override);

    Ok(())
}

#[test]
fn test_correct_non_avian_top_level_hierarchy_and_rename() -> anyhow::Result<()> {
    // Test hierarchy mode with top-level Scenery
    let dir = tempdir()?;
    let input_dir = dir.path().join("input");
    fs::create_dir_all(&input_dir)?;
    let img_path = input_dir.join("DSC_0002.JPG");
    fs::write(&img_path, b"scenery image payload")?;

    let output_dir = input_dir.join("birds_hierarchy");
    fs::create_dir_all(&output_dir)?;

    let db_path = output_dir.join(".avicull").join("manifest.sqlite");
    fs::create_dir_all(db_path.parent().unwrap())?;
    let db = ManifestDb::open_or_create(&db_path)?;

    let run_id = "run_hierarchy";
    db.record_run(run_id, &input_dir, &output_dir, false, "{}")?;

    let src_meta = fs::metadata(&img_path)?;
    let src_ino = src_meta.ino();

    let bundle = AssetBundle {
        bundle_id: "b2".to_string(),
        source_inode: src_ino,
        trip_folder_name: "trip".to_string(),
        original_stem: "DSC_0002".to_string(),
        primary_image: img_path.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0002.JPG")),
    };
    let dec = BundleDecision {
        bundle_id: "b2".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Bird, "House_Sparrow"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };
    db.record_bundles(run_id, &[dec], &[bundle])?;

    // Apply correction to top-level Scenery in hierarchy mode
    let res = apply_bundle_correction_with_category(
        &db,
        &output_dir,
        OrganizationMode::HierarchicalHardlinkHierarchy,
        "DSC_0002",
        "Scenery",
        "",
        Some(&input_dir),
        false,
    )?;

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.category(), &AssetCategory::Scenery);
    assert_eq!(res[0].0.entity_slug(), None);

    // Hard link should exist under output_dir/Scenery/DSC_0002*.JPG
    let scenery_dir = output_dir.join("Scenery");
    assert!(scenery_dir.exists(), "Scenery directory should exist");
    let entries: Vec<_> = fs::read_dir(&scenery_dir)?
        .map(|e| e.unwrap().path())
        .collect();
    let photo_link = entries
        .iter()
        .find(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("DSC_0002") && name.ends_with(".JPG")
        })
        .expect("Photo hardlink must exist under Scenery/");
    let link_meta = fs::metadata(photo_link)?;
    assert_eq!(link_meta.ino(), src_ino);

    // Test in-place-rename mode with top-level Scenery
    let rename_dir = dir.path().join("rename_test");
    fs::create_dir_all(&rename_dir)?;
    let rename_img = rename_dir.join("DSC_0003.JPG");
    fs::write(&rename_img, b"rename test payload")?;

    let r_db_path = rename_dir.join(".avicull").join("manifest.sqlite");
    fs::create_dir_all(r_db_path.parent().unwrap())?;
    let r_db = ManifestDb::open_or_create(&r_db_path)?;
    r_db.record_run("run_rename", &rename_dir, &rename_dir, false, "{}")?;

    let r_meta = fs::metadata(&rename_img)?;
    let r_bundle = AssetBundle {
        bundle_id: "b3".to_string(),
        source_inode: r_meta.ino(),
        trip_folder_name: "rename_test".to_string(),
        original_stem: "DSC_0003".to_string(),
        primary_image: rename_img.clone(),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("DSC_0003.JPG")),
    };
    let r_dec = BundleDecision {
        bundle_id: "b3".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        classification: Classification::granular(AssetCategory::Bird, "House_Sparrow"),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };
    r_db.record_bundles("run_rename", &[r_dec], &[r_bundle])?;

    let res_rename = apply_bundle_correction_with_category(
        &r_db,
        &rename_dir,
        OrganizationMode::InPlaceRename,
        "DSC_0003",
        "Scenery",
        "",
        Some(&rename_dir),
        false,
    )?;
    assert_eq!(res_rename.len(), 1);
    assert_eq!(res_rename[0].0.category(), &AssetCategory::Scenery);
    assert_eq!(res_rename[0].0.entity_slug(), None);

    // The renamed file should be DSC_0003_Scenery.JPG
    let expected_renamed = rename_dir.join("DSC_0003_Scenery.JPG");
    assert!(
        expected_renamed.exists(),
        "Expected renamed file at: {}",
        expected_renamed.display()
    );

    Ok(())
}
