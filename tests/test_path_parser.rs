// tests/test_path_parser.rs
use avicull::location::path_parser::{
    extract_candidate_location_tokens, is_structural_noise_token,
};
use std::path::Path;

#[test]
fn test_noise_token_detection() {
    assert!(is_structural_noise_token("raw"));
    assert!(is_structural_noise_token("RAW"));
    assert!(is_structural_noise_token("processed"));
    assert!(is_structural_noise_token("selects"));
    assert!(is_structural_noise_token("100canon"));
    assert!(is_structural_noise_token("2025-04-19"));
    assert!(is_structural_noise_token("apr 19 2026"));
    assert!(is_structural_noise_token("oct 2025"));
    assert!(!is_structural_noise_token("yellowstone"));
    assert!(!is_structural_noise_token("serengeti"));
    assert!(!is_structural_noise_token("monteverde"));
    assert!(!is_structural_noise_token("hokkaido"));
    assert!(!is_structural_noise_token("everglades"));
}

#[test]
fn test_global_photographer_archives_path_extraction() {
    // 1. Serengeti Migration in Africa
    let p1 = Path::new("Africa/2023-08_Serengeti_Migration/Selects/IMG_001.CR3");
    assert_eq!(
        extract_candidate_location_tokens(p1),
        vec!["serengeti migration", "africa"]
    );

    // 2. Yellowstone National Park
    let p2 = Path::new("Yellowstone 2024/Day 1/RAW/DSC001.ARW");
    assert_eq!(
        extract_candidate_location_tokens(p2),
        vec!["day 1", "yellowstone"]
    );

    // 3. Costa Rica Monteverde Cloud Forest
    let p3 = Path::new("Costa Rica/Monteverde Cloud Forest/Exports/photo.jpg");
    assert_eq!(
        extract_candidate_location_tokens(p3),
        vec!["monteverde cloud forest", "costa rica"]
    );

    // 4. Japan Hokkaido Winter
    let p4 = Path::new("Japan/Hokkaido/Winter 2022/Final/_DSC9999.NEF");
    assert_eq!(
        extract_candidate_location_tokens(p4),
        vec!["winter", "hokkaido", "japan"]
    );
}

#[test]
fn test_deeply_nested_noise_removal() {
    let path =
        Path::new("/Users/john/Pictures/2025/Yellowstone/Trip 1/RAW/Selects/100CANON/IMG_0001.CR2");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["trip 1", "yellowstone"]);
}

#[test]
fn test_date_stripping_and_delimiter_normalization() {
    let path = Path::new("/Volumes/Storage/2024-10-15_Everglades_National_Park/exports/photo.jpg");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["everglades national park"]);
}

#[test]
fn test_deduplication_of_tokens() {
    let path = Path::new("/Photos/Serengeti/pictures_serengeti/DSC_0001.NEF");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["serengeti"]);
}
