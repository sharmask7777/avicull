use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{OrganizationMode, RoutingTier, SpeciesPrediction};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn build_deterministic_catalog_items() -> Vec<CatalogItem> {
    vec![
        CatalogItem {
            bundle_id: "bundle_1".into(),
            original_stem: "1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore".into(),
            current_path: PathBuf::from("ranthambore oct 2025/processed/1X7A0806.jpg"),
            primary_image: PathBuf::from("ranthambore oct 2025/processed/1X7A0806.jpg"),
            companion_images: vec![],
            sidecar_files: vec![PathBuf::from(
                "ranthambore oct 2025/processed/1X7A0806.jpg.xmp",
            )],
            relative_preview_path: Some(".avicull/previews/1X7A0806.jpg".into()),
            is_local: Some(true),
            assigned_species: "Red-vented Bulbul".into(),
            scientific_name: Some("Pycnonotus cafer".into()),
            routing_tier: RoutingTier::ConfirmedSpecies,
            confidence: 0.94,
            capture_date: Some("2025-10-02".into()),
            location_name: Some("Ranthambore".into()),
            location_source: Some("trip_gazetteer".into()),
            subdirectory: Some("ranthambore oct 2025".into()),
            top_predictions: vec![
                SpeciesPrediction {
                    taxon_id: "redbul".into(),
                    common_name: "Red-vented Bulbul".into(),
                    folder_slug: "Red-vented_Bulbul".into(),
                    scientific_name: "Pycnonotus cafer".into(),
                    visual_prob: 0.94,
                    geo_prior: 1.0,
                    combined_score: 0.94,
                },
                SpeciesPrediction {
                    taxon_id: "bladro".into(),
                    common_name: "Black Drongo".into(),
                    folder_slug: "Black_Drongo".into(),
                    scientific_name: "Dicrurus macrocercus".into(),
                    visual_prob: 0.05,
                    geo_prior: 1.0,
                    combined_score: 0.05,
                },
            ],
        },
        CatalogItem {
            bundle_id: "bundle_2".into(),
            original_stem: "1X7A8099_Needs_Review_Purple_Finch_20251221_Coorg".into(),
            current_path: PathBuf::from("coorg Dec 2025/r7/processed/1X7A8099.jpg"),
            primary_image: PathBuf::from("coorg Dec 2025/r7/processed/1X7A8099.jpg"),
            companion_images: vec![],
            sidecar_files: vec![PathBuf::from(
                "coorg Dec 2025/r7/processed/1X7A8099.jpg.xmp",
            )],
            relative_preview_path: Some(".avicull/previews/1X7A8099.jpg".into()),
            is_local: Some(true),
            assigned_species: "Common Rosefinch".into(),
            scientific_name: Some("Carpodacus erythrinus".into()),
            routing_tier: RoutingTier::NeedsReview,
            confidence: 0.65,
            capture_date: Some("2025-12-21".into()),
            location_name: Some("Coorg".into()),
            location_source: Some("trip_gazetteer".into()),
            subdirectory: Some("coorg Dec 2025/r7".into()),
            top_predictions: vec![
                SpeciesPrediction {
                    taxon_id: "comros".into(),
                    common_name: "Common Rosefinch".into(),
                    folder_slug: "Common_Rosefinch".into(),
                    scientific_name: "Carpodacus erythrinus".into(),
                    visual_prob: 0.65,
                    geo_prior: 1.0,
                    combined_score: 0.65,
                },
                SpeciesPrediction {
                    taxon_id: "purfin".into(),
                    common_name: "Purple Finch".into(),
                    folder_slug: "Purple_Finch".into(),
                    scientific_name: "Haemorhous purpureus".into(),
                    visual_prob: 0.30,
                    geo_prior: 0.01,
                    combined_score: 0.003,
                },
            ],
        },
    ]
}

/// Normalizes nondeterministic runtime variables (such as temporary filesystem paths,
/// ISO execution timestamps, and local taxonomy caches) so snapshots can be reproducibly
/// asserted across runs and machines regardless of local model cache presence.
fn normalize_html_for_snapshot(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut remaining = html;

    while let Some(start_idx) = remaining.find("\"catalog_dir\":") {
        out.push_str(&remaining[..start_idx]);
        out.push_str("\"catalog_dir\":\"<NORMALIZED_CATALOG_DIR>\"");
        let rest = &remaining[start_idx + "\"catalog_dir\":".len()..];
        let quote_start = rest.find('"').unwrap_or(0);
        let quote_end = rest[quote_start + 1..].find('"').unwrap_or(0);
        remaining = &rest[quote_start + 1 + quote_end + 1..];
    }
    out.push_str(remaining);

    let mut out2 = String::with_capacity(out.len());
    remaining = &out2_src(&out);
    while let Some(start_idx) = remaining.find("\"generated_at\":") {
        out2.push_str(&remaining[..start_idx]);
        out2.push_str("\"generated_at\":\"<NORMALIZED_GENERATED_AT>\"");
        let rest = &remaining[start_idx + "\"generated_at\":".len()..];
        let quote_start = rest.find('"').unwrap_or(0);
        let quote_end = rest[quote_start + 1..].find('"').unwrap_or(0);
        remaining = &rest[quote_start + 1 + quote_end + 1..];
    }
    out2.push_str(remaining);

    let mut out3 = String::with_capacity(out2.len());
    remaining = &out2;
    while let Some(start_idx) = remaining.find("\"species_list\":") {
        out3.push_str(&remaining[..start_idx]);
        out3.push_str("\"species_list\":[\"<NORMALIZED_SPECIES_LIST>\"]");
        let rest = &remaining[start_idx + "\"species_list\":".len()..];
        if let Some(bracket_start) = rest.find('[') {
            if let Some(bracket_end) = rest[bracket_start..].find(']') {
                remaining = &rest[bracket_start + bracket_end + 1..];
            } else {
                remaining = "";
            }
        } else {
            remaining = "";
        }
    }
    out3.push_str(remaining);

    out3
}

fn out2_src(s: &str) -> &str {
    s
}

#[test]
fn test_catalog_raw_html_snapshot_parity() {
    let temp_root = tempdir().expect("create temp dir");
    let output_catalog_path = temp_root.path().join("bird_catalog.html");

    let items = build_deterministic_catalog_items();
    generate_html_catalog(
        "Avicull Snapshot Test Suite",
        &items,
        OrganizationMode::HierarchicalHardlinkHierarchy,
        &output_catalog_path,
    )
    .expect("failed to generate catalog html");

    let raw_html = fs::read_to_string(&output_catalog_path).expect("read generated catalog html");
    let normalized_html = normalize_html_for_snapshot(&raw_html);

    let snapshot_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/snapshots/bird_catalog_reference.snap");

    if !snapshot_path.exists() || std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        fs::create_dir_all(snapshot_path.parent().unwrap()).expect("create snapshot dir");
        fs::write(&snapshot_path, &normalized_html).expect("write reference snapshot");
        println!("Reference snapshot written to {:?}", snapshot_path);
    }

    let reference_html =
        fs::read_to_string(&snapshot_path).expect("failed to read reference snapshot html");

    // Check exact match between normalized HTML and reference snapshot
    if normalized_html != reference_html {
        for (i, (l1, l2)) in normalized_html
            .lines()
            .zip(reference_html.lines())
            .enumerate()
        {
            if l1 != l2 {
                eprintln!(
                    "Difference at line {}:\n  GEN: {}\n  REF: {}",
                    i + 1,
                    l1,
                    l2
                );
                break;
            }
        }
        panic!(
            "Catalog HTML snapshot mismatch!\nDifferences detected against {:?}.\nRun with UPDATE_SNAPSHOTS=1 to intentionally refresh snapshot baseline.",
            snapshot_path
        );
    }

    // Additional sanity assertions ensuring core UI elements are present in snapshot
    assert!(
        normalized_html.contains("id=\"btnMainReviewClassifications\""),
        "Snapshot must contain dedicated Review Classifications button"
    );
    assert!(
        normalized_html.contains("Location of the current directory:"),
        "Snapshot must contain explicit location prefix"
    );
    assert!(
        normalized_html.contains("id=\"btnModifyLocations\""),
        "Snapshot must contain dedicated Modify Locations button"
    );
}
