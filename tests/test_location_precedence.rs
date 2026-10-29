use anyhow::Result;
use avicull::location::resolver::ResolvedLocation;
use avicull::manifest::ManifestDb;
use avicull::types::LocationSource;
use tempfile::tempdir;

#[test]
fn test_bundle_location_sqlite_schema_and_crud() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path)?;

    // Initially none
    let none_loc = db.get_bundle_location("bundle_123")?;
    assert!(none_loc.is_none());

    let loc = ResolvedLocation {
        name: "Lalbagh Botanical Garden".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 12.9507,
        longitude: 77.5848,
        source: "ImageOverride".to_string(),
    };

    db.set_bundle_location("bundle_123", &loc)?;

    let fetched = db
        .get_bundle_location("bundle_123")?
        .expect("Found bundle location");
    assert_eq!(fetched.name, "Lalbagh Botanical Garden");
    assert_eq!(fetched.region_code, "IN-KA");
    assert!((fetched.latitude - 12.9507).abs() < 1e-4);
    assert!((fetched.longitude - 77.5848).abs() < 1e-4);

    // Verify LocationSource enum has ImageLocationOverride and SubdirectoryGazetteer
    let _source1 = LocationSource::ImageLocationOverride;
    let _source2 = LocationSource::SubdirectoryGazetteer;

    Ok(())
}

#[test]
fn test_hierarchical_directory_location_matching() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path)?;

    let parent_loc = ResolvedLocation {
        name: "Karnataka".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 15.3173,
        longitude: 75.7139,
        source: "KarnatakaParent".to_string(),
    };

    let sub_loc = ResolvedLocation {
        name: "Turahalli Forest".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 12.8856,
        longitude: 77.5342,
        source: "TurahalliSubdir".to_string(),
    };

    // Store parent location under "Trip_Bangalore"
    db.set_directory_location("Trip_Bangalore", &parent_loc)?;

    // Store subdirectory location under "Trip_Bangalore/Turahalli"
    db.set_directory_location("Trip_Bangalore/Turahalli", &sub_loc)?;

    // Subdirectory path "Trip_Bangalore/Turahalli" should match sub_loc exactly
    let matched_sub = db
        .get_hierarchical_directory_location(std::path::Path::new("Trip_Bangalore/Turahalli"))?
        .expect("Subdirectory match");
    assert_eq!(matched_sub.name, "Turahalli Forest");

    // Deeper subfolder "Trip_Bangalore/Turahalli/ZoneA" should inherit "Turahalli Forest" (closest ancestor)
    let matched_nested = db
        .get_hierarchical_directory_location(std::path::Path::new(
            "Trip_Bangalore/Turahalli/ZoneA",
        ))?
        .expect("Inherited subdirectory match");
    assert_eq!(matched_nested.name, "Turahalli Forest");

    // Sibling subfolder "Trip_Bangalore/CubbonPark" should fall back to parent "Trip_Bangalore"
    let matched_sibling = db
        .get_hierarchical_directory_location(std::path::Path::new("Trip_Bangalore/CubbonPark"))?
        .expect("Parent fallback match");
    assert_eq!(matched_sibling.name, "Karnataka");

    // Completely unmapped directory should return None
    let matched_none =
        db.get_hierarchical_directory_location(std::path::Path::new("UnrelatedTrip/Photos"))?;
    assert!(matched_none.is_none());

    Ok(())
}

#[test]
fn test_strict_four_tier_location_precedence() -> Result<()> {
    use avicull::location::resolver::apply_location_precedence;
    use avicull::types::{AssetBundle, GpsCoordinate, LocationSource};
    use std::path::PathBuf;

    let dir = tempdir()?;
    let db_path = dir.path().join("manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path)?;

    // 1. Bundle with embedded EXIF GPS
    let mut bundle_exif = AssetBundle {
        bundle_id: "bundle_exif".to_string(),
        source_inode: 101,
        trip_folder_name: "Trip_Bangalore".to_string(),
        original_stem: "IMG_0001".to_string(),
        primary_image: PathBuf::from("Trip_Bangalore/IMG_0001.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: Some(GpsCoordinate {
            latitude: 12.9716,
            longitude: 77.5946,
            source: LocationSource::ExifGps,
        }),
        resolved_location_name: Some("Bangalore City".to_string()),
        relative_path: Some(PathBuf::from("Trip_Bangalore/IMG_0001.CR2")),
    };

    // Store image override, subdirectory override, and parent directory override in DB
    let img_override = ResolvedLocation {
        name: "Custom Spot".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 12.1111,
        longitude: 77.1111,
        source: "ImageOverride".to_string(),
    };
    db.set_bundle_location("bundle_exif", &img_override)?;

    let subdir_override = ResolvedLocation {
        name: "Turahalli Subdir".to_string(),
        region_code: "IN-KA".to_string(),
        latitude: 12.2222,
        longitude: 77.2222,
        source: "SubdirOverride".to_string(),
    };
    db.set_directory_location("Trip_Bangalore", &subdir_override)?;

    // Apply precedence
    apply_location_precedence(&mut bundle_exif, &db)?;

    // Invariant 1: Embedded EXIF GPS MUST NOT be overridden by image, subdir, or parent override!
    assert_eq!(
        bundle_exif.gps_coords.unwrap().source,
        LocationSource::ExifGps
    );
    assert!((bundle_exif.gps_coords.unwrap().latitude - 12.9716).abs() < 1e-4);

    // 2. Bundle WITHOUT EXIF GPS, but WITH image-level override
    let mut bundle_img_override = AssetBundle {
        bundle_id: "bundle_no_exif".to_string(),
        source_inode: 102,
        trip_folder_name: "Trip_Bangalore".to_string(),
        original_stem: "IMG_0002".to_string(),
        primary_image: PathBuf::from("Trip_Bangalore/IMG_0002.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("Trip_Bangalore/IMG_0002.CR2")),
    };
    db.set_bundle_location("bundle_no_exif", &img_override)?;
    apply_location_precedence(&mut bundle_img_override, &db)?;

    // Invariant 2: Image override takes precedence over subdirectory & parent
    assert_eq!(
        bundle_img_override.gps_coords.unwrap().source,
        LocationSource::ImageLocationOverride
    );
    assert_eq!(
        bundle_img_override.resolved_location_name.as_deref(),
        Some("Custom Spot")
    );
    assert!((bundle_img_override.gps_coords.unwrap().latitude - 12.1111).abs() < 1e-4);

    // 3. Bundle WITHOUT EXIF, WITHOUT image override, inside Subdirectory
    let mut bundle_subdir = AssetBundle {
        bundle_id: "bundle_subdir".to_string(),
        source_inode: 103,
        trip_folder_name: "Trip_Bangalore".to_string(),
        original_stem: "IMG_0003".to_string(),
        primary_image: PathBuf::from("Trip_Bangalore/Turahalli/IMG_0003.CR2"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: Some(PathBuf::from("Trip_Bangalore/Turahalli/IMG_0003.CR2")),
    };
    db.set_directory_location("Trip_Bangalore/Turahalli", &subdir_override)?;
    apply_location_precedence(&mut bundle_subdir, &db)?;

    // Invariant 3: Subdirectory override is applied
    assert_eq!(
        bundle_subdir.gps_coords.unwrap().source,
        LocationSource::SubdirectoryGazetteer
    );
    assert_eq!(
        bundle_subdir.resolved_location_name.as_deref(),
        Some("Turahalli Subdir")
    );

    Ok(())
}
