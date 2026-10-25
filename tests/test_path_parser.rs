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
    assert!(!is_structural_noise_token("bangalore"));
    assert!(!is_structural_noise_token("saul kere"));
    assert!(!is_structural_noise_token("ranthambore"));
}

#[test]
fn test_extract_candidate_tokens_from_nested_path() {
    let path =
        Path::new("/Users/photos/pictures_bangalore-2025-/saul kere apr 19 2026/RAW/1X7A5242.CR2");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["saul kere", "bangalore"]);
}

#[test]
fn test_deeply_nested_noise_removal() {
    let path =
        Path::new("/Users/john/Pictures/2025/Bangalore/Trip 1/RAW/Selects/100CANON/IMG_0001.CR2");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["trip 1", "bangalore"]);
}

#[test]
fn test_date_stripping_and_delimiter_normalization() {
    let path = Path::new("/Volumes/Storage/2024-10-15_Ranthambore_National_Park/exports/photo.jpg");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["ranthambore national park"]);
}

#[test]
fn test_deduplication_of_tokens() {
    let path = Path::new("/Photos/Bangalore/pictures_bangalore/DSC_0001.NEF");
    let tokens = extract_candidate_location_tokens(path);
    assert_eq!(tokens, vec!["bangalore"]);
}
