use avicull::catalog::{generate_html_catalog, CatalogItem};
use avicull::types::{AssetCategory, OrganizationMode, RoutingTier};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_catalog_data_embeds_multi_taxa() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let catalog_path = dir.path().join("catalog.html");

    let item = CatalogItem {
        bundle_id: "b1".to_string(),
        original_stem: "DSC_0001".to_string(),
        current_path: PathBuf::from("DSC_0001.JPG"),
        primary_image: PathBuf::from("DSC_0001.JPG"),
        assigned_species: "Snow_Leopard".to_string(),
        scientific_name: Some("Panthera uncia".to_string()),
        confidence: 0.95,
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_local: Some(true),
        category: AssetCategory::Mammalia,
        ..Default::default()
    };

    generate_html_catalog(
        "Leh Trip",
        &[item],
        OrganizationMode::InPlace,
        &catalog_path,
    )?;

    let html = fs::read_to_string(&catalog_path)?;
    let json_start = html
        .find("<script id=\"catalog-data\" type=\"application/json\">")
        .expect("find json start")
        + "<script id=\"catalog-data\" type=\"application/json\">".len();
    let json_end = html[json_start..].find("</script>").expect("find json end") + json_start;
    let payload_val: serde_json::Value = serde_json::from_str(&html[json_start..json_end])?;

    // Invariant 1: species_list for birds remains populated (snapshot backward compatibility)
    let species_list = payload_val["species_list"]
        .as_array()
        .expect("species_list array");
    assert!(!species_list.is_empty());

    // Invariant 2: multi_taxa is populated with curated non-avian entries
    let multi_taxa = payload_val["multi_taxa"]
        .as_array()
        .expect("multi_taxa array");
    assert!(!multi_taxa.is_empty());

    let has_snow_leopard = multi_taxa.iter().any(|entry| {
        entry["category"] == "Mammalia" && entry["common_name"] == "Snow Leopard"
    });
    assert!(has_snow_leopard, "multi_taxa must include Snow Leopard under Mammalia");

    let has_reptilia = multi_taxa.iter().any(|entry| entry["category"] == "Reptilia");
    assert!(has_reptilia, "multi_taxa must include Reptilia category");

    let has_insecta = multi_taxa.iter().any(|entry| entry["category"] == "Insecta");
    assert!(has_insecta, "multi_taxa must include Insecta category");

    // Invariant 3: item category is Mammalia
    let items = payload_val["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["category"], "Mammalia");
    assert_eq!(items[0]["assigned_species"], "Snow_Leopard");

    Ok(())
}
