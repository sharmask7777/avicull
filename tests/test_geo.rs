use avicull::geo::*;
use avicull::types::*;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_geo_sibling_propagation() {
    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 1,
        trip_folder_name: "TripA".to_string(),
        original_stem: "IMG_1".to_string(),
        primary_image: PathBuf::from("IMG_1.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: Some(GpsCoordinate {
            latitude: 27.1,
            longitude: 77.5,
            source: LocationSource::ExifGps,
        }),
    };

    let b2 = AssetBundle {
        bundle_id: "b2".to_string(),
        source_inode: 2,
        trip_folder_name: "TripA".to_string(),
        original_stem: "IMG_2".to_string(),
        primary_image: PathBuf::from("IMG_2.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None, // Missing GPS!
    };

    let mut bundles = vec![b1, b2];
    resolve_locations(&mut bundles, None, None).expect("resolve_locations failed");

    assert_eq!(
        bundles[0].gps_coords.unwrap().source,
        LocationSource::ExifGps
    );
    assert_eq!(
        bundles[1].gps_coords.unwrap().source,
        LocationSource::SiblingPropagation
    );
    assert!((bundles[1].gps_coords.unwrap().latitude - 27.1).abs() < 1e-4);
    assert!((bundles[1].gps_coords.unwrap().longitude - 77.5).abs() < 1e-4);
}

#[test]
fn test_geo_trip_folder_gazetteer_toml() {
    let dir = tempdir().expect("create temp dir");
    let toml_path = dir.path().join("locations.toml");
    let mut f = File::create(&toml_path).unwrap();
    writeln!(
        f,
        r#"
[ranganathittu]
lat = 12.4244
lon = 76.6563
"#
    )
    .unwrap();

    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 1,
        trip_folder_name: "2025-12_Ranganathittu_Trip".to_string(),
        original_stem: "IMG_1".to_string(),
        primary_image: PathBuf::from("IMG_1.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
    };

    let mut bundles = vec![b1];
    resolve_locations(&mut bundles, Some(&toml_path), None).expect("resolve_locations failed");

    let coords = bundles[0]
        .gps_coords
        .expect("Should resolve coords via toml gazetteer");
    assert_eq!(coords.source, LocationSource::TripFolderGazetteer);
    assert!((coords.latitude - 12.4244).abs() < 1e-4);
    assert!((coords.longitude - 76.6563).abs() < 1e-4);
}

#[test]
fn test_geo_embedded_gazetteer_fallback() {
    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 1,
        trip_folder_name: "2024-08_Ladakh_Leh_Birds".to_string(),
        original_stem: "IMG_1".to_string(),
        primary_image: PathBuf::from("IMG_1.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
    };

    let mut bundles = vec![b1];
    resolve_locations(&mut bundles, None, None).expect("resolve_locations failed");

    let coords = bundles[0]
        .gps_coords
        .expect("Should resolve coords via built-in gazetteer");
    assert_eq!(coords.source, LocationSource::TripFolderGazetteer);
    assert!((coords.latitude - 34.1526).abs() < 0.5); // Leh coordinates ~ 34.15 N, 77.57 E
}

#[test]
fn test_geo_default_fallback_coordinates() {
    let b1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 1,
        trip_folder_name: "Unknown_Somewhere_Unknown".to_string(),
        original_stem: "IMG_1".to_string(),
        primary_image: PathBuf::from("IMG_1.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
    };

    let mut bundles = vec![b1];
    resolve_locations(&mut bundles, None, Some((10.0, 20.0))).expect("resolve_locations failed");

    let coords = bundles[0]
        .gps_coords
        .expect("Should resolve coords via default fallback");
    assert_eq!(coords.source, LocationSource::UserDefaultRegion);
    assert_eq!(coords.latitude, 10.0);
    assert_eq!(coords.longitude, 20.0);
}
