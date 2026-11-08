//! # Tests for WebDAV Loopback Projection Engine & In-Memory Taxonomic Tree
//!
//! Verifies:
//! - In-memory taxonomic prefix tree construction from bundle records.
//! - O(1) path resolution of virtual taxonomic folders (`/Aves/<Order>/<Family>/<Species>/<file>`).
//! - Fast 404 response for macOS Finder metadata probes (`.DS_Store`, `._*`).

use avicull::storage::webdav_server::{TaxonomicNode, TaxonomicTree};

#[test]
fn test_taxonomic_prefix_tree_path_lookup() {
    let mut tree = TaxonomicTree::new();

    tree.insert(&avicull::storage::webdav_server::VirtualAssetEntry {
        order: "Passeriformes",
        family: "Corvidae",
        species_slug: "Corvus_corax",
        common_name: "Common Raven",
        filename: "2023-05-12_DSC0421.CR2",
        source_uri: "s3://my-bucket/2023-05-12_DSC0421.CR2",
        size_bytes: 45_000_000,
    });

    // Root listing
    let root_children = tree.list_directory("/").unwrap();
    assert!(root_children.contains(&"Aves".to_string()));

    // Order listing
    let aves_children = tree.list_directory("/Aves").unwrap();
    assert!(aves_children.contains(&"Passeriformes".to_string()));

    // Family listing
    let passer_children = tree.list_directory("/Aves/Passeriformes").unwrap();
    assert!(passer_children.contains(&"Corvidae".to_string()));

    // Species listing
    let corvidae_children = tree.list_directory("/Aves/Passeriformes/Corvidae").unwrap();
    assert!(corvidae_children.contains(&"Corvus_corax".to_string()));

    // Leaf file listing
    let species_children = tree
        .list_directory("/Aves/Passeriformes/Corvidae/Corvus_corax")
        .unwrap();
    assert!(species_children.contains(&"2023-05-12_DSC0421.CR2".to_string()));

    // Look up file metadata
    let node = tree
        .resolve("/Aves/Passeriformes/Corvidae/Corvus_corax/2023-05-12_DSC0421.CR2")
        .unwrap();
    match node {
        TaxonomicNode::File {
            common_name,
            size_bytes,
            ..
        } => {
            assert_eq!(common_name, "Common Raven");
            assert_eq!(*size_bytes, 45_000_000);
        }
        _ => panic!("Expected leaf file node"),
    }
}

#[test]
fn test_webdav_metadata_probe_silencing() {
    assert!(TaxonomicTree::is_macos_metadata_noise("/.DS_Store"));
    assert!(TaxonomicTree::is_macos_metadata_noise(
        "/Aves/._DSC0421.CR2"
    ));
    assert!(TaxonomicTree::is_macos_metadata_noise(
        "/.Spotlight-V100/Store-V1"
    ));
    assert!(!TaxonomicTree::is_macos_metadata_noise(
        "/Aves/Passeriformes/Corvidae"
    ));
}
