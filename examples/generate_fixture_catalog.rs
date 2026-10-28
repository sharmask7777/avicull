use avicull::catalog::template::render_catalog_html;
use avicull::catalog::{CatalogPayload, CatalogPayloadItem, CatalogStats, PayloadPrediction};
use std::fs;
use std::path::Path;

fn make_svg_data_uri(species: &str, color: &str) -> String {
    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='320' height='240'><rect width='100%' height='100%' fill='{}'/><text x='50%' y='50%' dominant-baseline='middle' text-anchor='middle' fill='%23ffffff' font-size='18' font-family='sans-serif'>{}</text></svg>",
        color, species
    )
}

fn create_payload() -> CatalogPayload {
    let items = vec![
        CatalogPayloadItem {
            id: "bundle-1".to_string(),
            original_stem: "IMG_1001".to_string(),
            current_path: "IMG_1001_House_Sparrow_20261025_Leh.CR2".to_string(),
            primary_filename: "IMG_1001.CR2".to_string(),
            preview_url: make_svg_data_uri("House Sparrow", "%23059669"),
            local_src: make_svg_data_uri("House Sparrow", "%23059669"),
            thumb_src: make_svg_data_uri("House Sparrow", "%23059669"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("House Sparrow", "%23059669")),
            is_local: true,
            assigned_species: "House Sparrow".to_string(),
            scientific_name: Some("Passer domesticus".to_string()),
            routing_tier: "ConfirmedSpecies".to_string(),
            confidence: 0.95,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Leh Palace".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec!["IMG_1001_House_Sparrow_20261025_Leh.JPG".to_string()],
            sidecar_files: vec!["IMG_1001_House_Sparrow_20261025_Leh.CR2.xmp".to_string()],
            top_predictions: vec![PayloadPrediction {
                common_name: "House Sparrow".to_string(),
                scientific_name: "Passer domesticus".to_string(),
                score: 0.95,
            }],
        },
        CatalogPayloadItem {
            id: "bundle-2".to_string(),
            original_stem: "IMG_1002".to_string(),
            current_path: "IMG_1002_Eurasian_Tree_Sparrow_20261025_Leh.CR2".to_string(),
            primary_filename: "IMG_1002.CR2".to_string(),
            preview_url: make_svg_data_uri("Eurasian Tree Sparrow", "%230d9488"),
            local_src: make_svg_data_uri("Eurasian Tree Sparrow", "%230d9488"),
            thumb_src: make_svg_data_uri("Eurasian Tree Sparrow", "%230d9488"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("Eurasian Tree Sparrow", "%230d9488")),
            is_local: true,
            assigned_species: "Eurasian Tree Sparrow".to_string(),
            scientific_name: Some("Passer montanus".to_string()),
            routing_tier: "ConfirmedSpecies".to_string(),
            confidence: 0.91,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Shey Marshes".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec!["IMG_1002_Eurasian_Tree_Sparrow_20261025_Leh.CR2.xmp".to_string()],
            top_predictions: vec![PayloadPrediction {
                common_name: "Eurasian Tree Sparrow".to_string(),
                scientific_name: "Passer montanus".to_string(),
                score: 0.91,
            }],
        },
        CatalogPayloadItem {
            id: "bundle-3".to_string(),
            original_stem: "IMG_1003".to_string(),
            current_path: "IMG_1003_Needs_Review_Coppersmith_Barbet_20261025_Delhi.CR2".to_string(),
            primary_filename: "IMG_1003.CR2".to_string(),
            preview_url: make_svg_data_uri("Coppersmith Barbet", "%23d97706"),
            local_src: make_svg_data_uri("Coppersmith Barbet", "%23d97706"),
            thumb_src: make_svg_data_uri("Coppersmith Barbet", "%23d97706"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("Coppersmith Barbet", "%23d97706")),
            is_local: true,
            assigned_species: "Coppersmith Barbet".to_string(),
            scientific_name: Some("Psilopogon haemacephalus".to_string()),
            routing_tier: "NeedsReview".to_string(),
            confidence: 0.58,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Lodhi Gardens".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![PayloadPrediction {
                common_name: "Coppersmith Barbet".to_string(),
                scientific_name: "Psilopogon haemacephalus".to_string(),
                score: 0.58,
            }],
        },
        CatalogPayloadItem {
            id: "bundle-4".to_string(),
            original_stem: "IMG_1004".to_string(),
            current_path: "IMG_1004_Great_Barbet_20261025_Pangot.CR2".to_string(),
            primary_filename: "IMG_1004.CR2".to_string(),
            preview_url: make_svg_data_uri("Great Barbet", "%232563eb"),
            local_src: make_svg_data_uri("Great Barbet", "%232563eb"),
            thumb_src: make_svg_data_uri("Great Barbet", "%232563eb"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("Great Barbet", "%232563eb")),
            is_local: true,
            assigned_species: "Great Barbet".to_string(),
            scientific_name: Some("Psilopogon virens".to_string()),
            routing_tier: "ConfirmedSpecies".to_string(),
            confidence: 0.92,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Pangot".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![PayloadPrediction {
                common_name: "Great Barbet".to_string(),
                scientific_name: "Psilopogon virens".to_string(),
                score: 0.92,
            }],
        },
        CatalogPayloadItem {
            id: "bundle-5".to_string(),
            original_stem: "IMG_1005".to_string(),
            current_path: "IMG_1005_Needs_Review_Black_Redstart_20261025_Leh.CR2".to_string(),
            primary_filename: "IMG_1005.CR2".to_string(),
            preview_url: make_svg_data_uri("Black Redstart", "%23b45309"),
            local_src: make_svg_data_uri("Black Redstart", "%23b45309"),
            thumb_src: make_svg_data_uri("Black Redstart", "%23b45309"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("Black Redstart", "%23b45309")),
            is_local: true,
            assigned_species: "Black Redstart".to_string(),
            scientific_name: Some("Phoenicurus ochruros".to_string()),
            routing_tier: "NeedsReview".to_string(),
            confidence: 0.49,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Thiksey".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![PayloadPrediction {
                common_name: "Black Redstart".to_string(),
                scientific_name: "Phoenicurus ochruros".to_string(),
                score: 0.49,
            }],
        },
        CatalogPayloadItem {
            id: "bundle-6".to_string(),
            original_stem: "IMG_1006".to_string(),
            current_path: "IMG_1006_No_Bird_Detected_20261025_Leh.CR2".to_string(),
            primary_filename: "IMG_1006.CR2".to_string(),
            preview_url: make_svg_data_uri("No Bird Detected", "%23475569"),
            local_src: make_svg_data_uri("No Bird Detected", "%23475569"),
            thumb_src: make_svg_data_uri("No Bird Detected", "%23475569"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("No Bird Detected", "%23475569")),
            is_local: true,
            assigned_species: "_No_Bird_Detected".to_string(),
            scientific_name: None,
            routing_tier: "NoBirdDetected".to_string(),
            confidence: 0.10,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Leh Highway".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![],
        },
        CatalogPayloadItem {
            id: "bundle-7".to_string(),
            original_stem: "IMG_1007".to_string(),
            current_path: "IMG_1007_No_Bird_Detected_20261025_Nubra.CR2".to_string(),
            primary_filename: "IMG_1007.CR2".to_string(),
            preview_url: make_svg_data_uri("No Bird Detected", "%23475569"),
            local_src: make_svg_data_uri("No Bird Detected", "%23475569"),
            thumb_src: make_svg_data_uri("No Bird Detected", "%23475569"),
            embedded_thumb_data_uri: Some(make_svg_data_uri("No Bird Detected", "%23475569")),
            is_local: true,
            assigned_species: "_No_Bird_Detected".to_string(),
            scientific_name: None,
            routing_tier: "NoBirdDetected".to_string(),
            confidence: 0.05,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Nubra Valley".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![],
        },
        CatalogPayloadItem {
            id: "bundle-8".to_string(),
            original_stem: "IMG_1008".to_string(),
            current_path: "IMG_1008_White_throated_Kingfisher_20261025_Okhla.CR2".to_string(),
            primary_filename: "IMG_1008.CR2".to_string(),
            preview_url: make_svg_data_uri("White-throated Kingfisher", "%230891b2"),
            local_src: make_svg_data_uri("White-throated Kingfisher", "%230891b2"),
            thumb_src: make_svg_data_uri("White-throated Kingfisher", "%230891b2"),
            embedded_thumb_data_uri: Some(make_svg_data_uri(
                "White-throated Kingfisher",
                "%230891b2",
            )),
            is_local: true,
            assigned_species: "White-throated Kingfisher".to_string(),
            scientific_name: Some("Halcyon smyrnensis".to_string()),
            routing_tier: "ConfirmedSpecies".to_string(),
            confidence: 0.97,
            capture_date: Some("2026-10-25".to_string()),
            location_name: Some("Okhla Sanctuary".to_string()),
            location_source: None,
            subdirectory: None,
            companion_files: vec![],
            sidecar_files: vec![],
            top_predictions: vec![PayloadPrediction {
                common_name: "White-throated Kingfisher".to_string(),
                scientific_name: "Halcyon smyrnensis".to_string(),
                score: 0.97,
            }],
        },
    ];

    let species_list = vec![
        "Asian Openbill".to_string(),
        "Barn Owl".to_string(),
        "Black Redstart".to_string(),
        "Black-necked Crane".to_string(),
        "Common Kingfisher".to_string(),
        "Common Redstart".to_string(),
        "Coppersmith Barbet".to_string(),
        "Eurasian Tree Sparrow".to_string(),
        "Great Barbet".to_string(),
        "House Sparrow".to_string(),
        "No Bird Detected".to_string(),
        "Painted Stork".to_string(),
        "White-throated Kingfisher".to_string(),
    ];

    let stats = CatalogStats {
        total_photos: 8,
        species_count: 6,
        confirmed_count: 4,
        needs_review_count: 2,
        no_bird_count: 2,
    };

    CatalogPayload {
        trip_name: "Himalayan & Plains Birding 2026".to_string(),
        mode: "in-place".to_string(),
        catalog_dir: "/mock/photos/birds_hierarchy".to_string(),
        trip_location_name: Some("Leh, Ladakh".to_string()),
        generated_at: "2026-10-28T09:00:00Z".to_string(),
        stats,
        items,
        species_list,
    }
}

pub fn generate_fixture(target_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let payload = create_payload();
    let html = render_catalog_html(&payload);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_path, html)?;
    println!("Successfully generated fixture at: {:?}", target_path);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root_fixture = Path::new("fixtures/test_bird_catalog.html");
    generate_fixture(root_fixture)?;

    let e2e_fixture = Path::new("tests/e2e/fixtures/test_bird_catalog.html");
    generate_fixture(e2e_fixture)?;

    Ok(())
}
