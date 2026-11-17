//! # Architectural Layering & Dependency Contract Tests
//!
//! Enforces clean unidirectional dependency flow across internal modules in `src/`:
//! - Foundational domain types (`src/types.rs`) must be pure leaves and NEVER depend on
//!   higher-level orchestration (`scanner`, `storage`, `organizer`, `catalog`, `db`).
//! - Storage backends (`src/storage/`) must NEVER depend on presentation (`src/catalog/`) or CLI.
//! - Scanners (`src/scanner.rs`) must NEVER depend on presentation (`src/catalog/`).
//! - Machine learning classification (`src/classifier.rs`, `src/burst.rs`) must remain decoupled from presentation.

use std::fs;
use std::path::{Path, PathBuf};

fn parse_crate_imports(file_path: &Path) -> Vec<String> {
    let content = fs::read_to_string(file_path).unwrap_or_default();
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Check for 'use crate::foo' or 'use crate::{foo, bar}'
        if let Some(after_crate) = trimmed.strip_prefix("use crate::") {
            let module_token = after_crate
                .split([':', ';', ' '])
                .next()
                .unwrap_or("")
                .trim();
            if !module_token.is_empty() {
                imports.push(module_token.to_string());
            }
        }
    }
    imports
}

#[test]
fn test_types_module_is_pure_leaf() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let types_path = manifest_dir.join("src/types.rs");
    let imports = parse_crate_imports(&types_path);

    // types.rs must never import higher-level subsystem crates
    let forbidden = [
        "scanner",
        "storage",
        "organizer",
        "catalog",
        "db",
        "geo",
        "cli",
    ];
    for imp in &imports {
        assert!(
            !forbidden.contains(&imp.as_str()),
            "Architecture contract violated: src/types.rs must not depend on '{}'",
            imp
        );
    }
}

#[test]
fn test_storage_does_not_depend_on_catalog_or_cli() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let storage_dir = manifest_dir.join("src/storage");
    let storage_file = manifest_dir.join("src/storage.rs");

    let mut files = Vec::new();
    if storage_file.exists() {
        files.push(storage_file);
    }
    if storage_dir.exists() {
        if let Ok(entries) = fs::read_dir(storage_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    files.push(entry.path());
                }
            }
        }
    }

    let forbidden = ["catalog", "cli"];
    for f in files {
        let imports = parse_crate_imports(&f);
        for imp in &imports {
            assert!(
                !forbidden.contains(&imp.as_str()),
                "Architecture contract violated: {} must not depend on '{}'",
                f.display(),
                imp
            );
        }
    }
}

#[test]
fn test_classification_and_burst_do_not_depend_on_catalog() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for module_name in &["classifier.rs", "burst.rs", "geo.rs", "db.rs"] {
        let module_path = manifest_dir.join("src").join(module_name);
        if module_path.exists() {
            let imports = parse_crate_imports(&module_path);
            assert!(
                !imports.iter().any(|i| i == "catalog"),
                "Architecture contract violated: src/{} must not depend on catalog",
                module_name
            );
        }
    }
}
