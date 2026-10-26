use crate::catalog::{generate_html_catalog, CatalogItem};
use crate::db::ManifestDb;
use crate::location::trigram_similarity;
use crate::organizer::inplace::{execute_inplace_bundle_rename_with_stem, format_inplace_filename};
use crate::organizer::{compute_target_dir, materialize_bundle_hardlinks, sanitize_species_slug};
use crate::storage::detect_storage_backend;
use crate::types::{AssetBundle, BundleDecision, OrganizationMode, RoutingTier};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Authoritative record from eBird / AviList taxonomy for typo matching and suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyEntry {
    pub common_name: String,
    pub folder_slug: String,
    pub scientific_name: Option<String>,
}

/// A fuzzy match suggestion against the eBird taxonomy.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesTypoSuggestion {
    pub common_name: String,
    pub folder_slug: String,
    pub similarity: f64,
}

/// Checks if standard I/O is interactive or forced via the test environment variable.
fn is_interactive_session() -> bool {
    (std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
        || std::env::var("AVICULL_FORCE_INTERACTIVE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

/// Loads eBird taxonomy records from cached or local models, falling back to a curated global list.
pub fn load_taxonomy_records() -> Vec<TaxonomyEntry> {
    let mut candidate_paths = vec![
        PathBuf::from("models/taxonomy.json"),
        PathBuf::from("../models/taxonomy.json"),
        PathBuf::from("tests/fixtures/models/taxonomy.json"),
    ];

    if let Ok(cargo_manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        candidate_paths.push(
            PathBuf::from(cargo_manifest)
                .join("tests")
                .join("fixtures")
                .join("models")
                .join("taxonomy.json"),
        );
    }

    if let Ok(models_dir) = std::env::var("AVICULL_MODELS_DIR") {
        candidate_paths.push(PathBuf::from(models_dir).join("taxonomy.json"));
    }

    if let Ok(home) = std::env::var("HOME") {
        candidate_paths.push(
            PathBuf::from(home)
                .join(".cache")
                .join("avicull")
                .join("models")
                .join("taxonomy.json"),
        );
    }

    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in candidate_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    for obj in parsed {
                        if let Some(common_name) = obj.get("common_name").and_then(|v| v.as_str()) {
                            let folder_slug = obj
                                .get("folder_slug")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| sanitize_species_slug(common_name));
                            let scientific_name = obj
                                .get("scientific_name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            if seen.insert(common_name.to_lowercase()) {
                                entries.push(TaxonomyEntry {
                                    common_name: common_name.to_string(),
                                    folder_slug,
                                    scientific_name,
                                });
                            }
                        }
                    }
                    if !entries.is_empty() {
                        break;
                    }
                }
            }
        }
    }

    for fallback in curated_taxonomy_fallback() {
        if seen.insert(fallback.common_name.to_lowercase()) {
            entries.push(fallback);
        }
    }

    entries
}

/// Fallback list of prominent bird species across world families when taxonomy.json is unavailable.
fn curated_taxonomy_fallback() -> Vec<TaxonomyEntry> {
    vec![
        "House Sparrow",
        "Eurasian Tree Sparrow",
        "Black Redstart",
        "Common Redstart",
        "Tibetan Snowcock",
        "Himalayan Snowcock",
        "Lammergeier",
        "Golden Eagle",
        "Steppe Eagle",
        "Painted Stork",
        "Asian Openbill",
        "Black-necked Crane",
        "Common Crane",
        "Bar-headed Goose",
        "Ruddy Shelduck",
        "Mallard",
        "Common Kingfisher",
        "White-throated Kingfisher",
        "Pied Kingfisher",
        "Little Egret",
        "Great Egret",
        "Grey Heron",
        "Peregrine Falcon",
        "Common Kestrel",
        "Barn Owl",
        "Spotted Owlet",
        "Eurasian Eagle-Owl",
        "Blue Rock Thrush",
        "Great Tit",
        "Coal Tit",
        "Eurasian Magpie",
        "Red-billed Chough",
        "Yellow-billed Chough",
        "Brown-headed Gull",
        "Common Sandpiper",
        "Red-wattled Lapwing",
        "Little Ringed Plover",
    ]
    .into_iter()
    .map(|name| TaxonomyEntry {
        common_name: name.to_string(),
        folder_slug: sanitize_species_slug(name),
        scientific_name: None,
    })
    .collect()
}

/// Identifies whether an input string is a probable typo for an authoritative taxonomy species
/// using character trigram similarity. Returns None if the input is already recognized as an
/// exact common name, folder slug, or scientific name.
pub fn find_species_typo_suggestion(
    input: &str,
    taxonomy: &[TaxonomyEntry],
) -> Option<SpeciesTypoSuggestion> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "_No_Bird_Detected" || trimmed == "NoBirdDetected" {
        return None;
    }

    let input_norm = trimmed.replace('_', " ");

    for entry in taxonomy {
        if entry.common_name.eq_ignore_ascii_case(trimmed)
            || entry.common_name.eq_ignore_ascii_case(&input_norm)
            || entry.folder_slug.eq_ignore_ascii_case(trimmed)
            || entry
                .scientific_name
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(trimmed))
                .unwrap_or(false)
        {
            return None;
        }
    }

    let mut best_entry: Option<(&TaxonomyEntry, f64)> = None;

    for entry in taxonomy {
        let sim_common = trigram_similarity(&input_norm, &entry.common_name);
        let sim_slug = trigram_similarity(trimmed, &entry.folder_slug);
        let sim_sci = entry
            .scientific_name
            .as_deref()
            .map(|s| trigram_similarity(trimmed, s))
            .unwrap_or(0.0);

        let sim = sim_common.max(sim_slug).max(sim_sci);

        if sim >= 0.60 {
            match best_entry {
                Some((_, best_sim)) if sim > best_sim => {
                    best_entry = Some((entry, sim));
                }
                None => {
                    best_entry = Some((entry, sim));
                }
                _ => {}
            }
        }
    }

    best_entry.map(|(entry, sim)| SpeciesTypoSuggestion {
        common_name: entry.common_name.clone(),
        folder_slug: entry.folder_slug.clone(),
        similarity: sim,
    })
}

/// Applies a direct manual classification correction for a single photo bundle or burst.
///
/// Updates the bundle's classification and routing tier in the SQLite manifest,
/// marks it as a durable user override (`is_user_override = 1`, `final_confidence = 1.0`),
/// moves or renames all associated files (primary image, companions, and sidecars)
/// according to the active organization mode, and regenerates `bird_catalog.html`.
/// When `propagate_burst` is enabled or confirmed interactively, all frames belonging
/// to the same burst are reclassified and relocated lockstep.
pub fn apply_bundle_correction(
    db: &ManifestDb,
    operating_root: &Path,
    mode: OrganizationMode,
    file_query: &str,
    new_species: &str,
    input_root: Option<&Path>,
    propagate_burst: bool,
) -> Result<Vec<(BundleDecision, AssetBundle)>> {
    let default_root = PathBuf::from(".");
    let operating_root = if operating_root.as_os_str().is_empty() {
        &default_root
    } else {
        operating_root
    };
    let query_trim = file_query.trim();
    let clean_query = query_trim.strip_prefix("./").unwrap_or(query_trim);
    let has_path_separator = clean_query.contains('/') || clean_query.contains('\\');

    let query_stem = Path::new(clean_query)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(clean_query);
    let query_file_name = Path::new(clean_query)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(clean_query);

    let all_bundles = db.get_all_bundles(None)?;

    // Filter all bundles matching the query by bundle ID, path, or filename/stem
    let mut matched_bundles: Vec<(BundleDecision, AssetBundle)> = all_bundles
        .iter()
        .filter(|(_, bundle)| {
            if bundle.bundle_id == clean_query {
                return true;
            }

            if has_path_separator {
                let target_path = Path::new(clean_query);
                if let Some(ref rel) = bundle.relative_path {
                    if rel == target_path || rel.ends_with(target_path) {
                        return true;
                    }
                    if rel.with_extension("") == target_path {
                        return true;
                    }
                    if rel.with_extension("") == target_path.with_extension("")
                        && rel.parent() == target_path.parent()
                    {
                        return true;
                    }
                }
                if bundle.primary_image.ends_with(target_path)
                    || bundle
                        .primary_image
                        .with_extension("")
                        .ends_with(target_path)
                {
                    return true;
                }
                for comp in &bundle.companion_images {
                    if comp.ends_with(target_path) || comp.with_extension("").ends_with(target_path)
                    {
                        return true;
                    }
                }
                for sc in &bundle.sidecar_files {
                    if sc.ends_with(target_path) {
                        return true;
                    }
                }
                return false;
            }

            if bundle.original_stem == clean_query || bundle.original_stem == query_stem {
                return true;
            }
            if let Some(name) = bundle.primary_image.file_name().and_then(|n| n.to_str()) {
                if name == query_file_name || name == clean_query {
                    return true;
                }
            }
            if let Some(stem) = bundle.primary_image.file_stem().and_then(|s| s.to_str()) {
                if stem == query_stem {
                    return true;
                }
            }
            for comp in &bundle.companion_images {
                if let Some(name) = comp.file_name().and_then(|n| n.to_str()) {
                    if name == query_file_name || name == clean_query {
                        return true;
                    }
                }
                if let Some(stem) = comp.file_stem().and_then(|s| s.to_str()) {
                    if stem == query_stem {
                        return true;
                    }
                }
            }
            for sc in &bundle.sidecar_files {
                if let Some(name) = sc.file_name().and_then(|n| n.to_str()) {
                    if name == query_file_name || name == clean_query {
                        return true;
                    }
                }
            }
            if let Some(ref rel) = bundle.relative_path {
                if rel == Path::new(clean_query) || rel.ends_with(clean_query) {
                    return true;
                }
            }
            false
        })
        .cloned()
        .collect();

    let (target_dec, target_bundle) = match matched_bundles.len() {
        0 => {
            return Err(anyhow!(
                "No photo bundle matching '{}' was found in manifest.",
                file_query
            ));
        }
        1 => matched_bundles.remove(0),
        _ => {
            let is_interactive = is_interactive_session();
            if is_interactive {
                println!("\nMultiple photo bundles matched query '{}':", file_query);
                for (i, (dec, b)) in matched_bundles.iter().enumerate() {
                    let rel = b
                        .relative_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| b.primary_image.to_string_lossy().to_string());
                    let date = b
                        .capture_timestamp_ms
                        .and_then(|ms| {
                            chrono::DateTime::from_timestamp_millis(ms)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        })
                        .unwrap_or_else(|| "unknown date".to_string());
                    println!(
                        "  [{}] {} (trip: {}, date: {}, current: {})",
                        i + 1,
                        rel,
                        b.trip_folder_name,
                        date,
                        dec.assigned_species_slug
                    );
                }
                print!(
                    "Select bundle to reclassify [1-{}]: ",
                    matched_bundles.len()
                );
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let choice: usize = input
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!("Invalid selection '{}'. Aborted.", input.trim()))?;
                if choice < 1 || choice > matched_bundles.len() {
                    return Err(anyhow!(
                        "Selection {} out of range [1-{}].",
                        choice,
                        matched_bundles.len()
                    ));
                }
                matched_bundles.remove(choice - 1)
            } else {
                let mut msg = format!("Multiple photo bundles matched query '{}':\n", file_query);
                for (_dec, b) in &matched_bundles {
                    let rel = b
                        .relative_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| b.primary_image.to_string_lossy().to_string());
                    msg.push_str(&format!("  - {}\n", rel));
                }
                msg.push_str("Specify an unambiguous relative path using --file (e.g. --file \"<trip>/<file>\").");
                return Err(anyhow!(msg));
            }
        }
    };

    // Identify burst members if the bundle is part of a multi-frame burst
    let burst_members: Vec<(BundleDecision, AssetBundle)> =
        if let Some(ref b_id) = target_dec.burst_id {
            all_bundles
                .iter()
                .filter(|(d, b)| {
                    d.burst_id.as_deref() == Some(b_id)
                        && b.trip_folder_name == target_bundle.trip_folder_name
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

    let mut should_propagate = propagate_burst;
    if burst_members.len() > 1 && !should_propagate {
        let is_interactive = is_interactive_session();
        if is_interactive {
            let b_id = target_dec.burst_id.as_deref().unwrap_or("burst");
            print!(
                "\nPhoto is part of a {}-photo burst ({}). Reclassify entire burst? [y/N]: ",
                burst_members.len(),
                b_id
            );
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let trimmed = input.trim();
            if trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes") {
                should_propagate = true;
            }
        }
    }

    let bundles_to_update = if should_propagate && burst_members.len() > 1 {
        burst_members
    } else {
        vec![(target_dec, target_bundle)]
    };

    let mut resolved_species = new_species.to_string();
    let (has_needs_review, species_to_check) =
        if let Some(stripped) = new_species.strip_prefix("_Needs_Review/") {
            (true, stripped)
        } else {
            (false, new_species)
        };

    if species_to_check != "_No_Bird_Detected" && species_to_check != "NoBirdDetected" {
        let taxonomy = load_taxonomy_records();
        if let Some(suggestion) = find_species_typo_suggestion(species_to_check, &taxonomy) {
            if is_interactive_session() {
                print!(
                    "\nSpecies '{}' not found in eBird taxonomy. Did you mean '{}'? [Y/n]: ",
                    species_to_check, suggestion.common_name
                );
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let trimmed = input.trim();
                if trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("y")
                    || trimmed.eq_ignore_ascii_case("yes")
                {
                    resolved_species = if has_needs_review {
                        format!("_Needs_Review/{}", suggestion.common_name)
                    } else {
                        suggestion.common_name
                    };
                }
            } else {
                println!(
                    "[avicull] Note: Species '{}' not found in taxonomy. Did you mean '{}'?",
                    species_to_check, suggestion.common_name
                );
            }
        }
    }

    let (routing_tier, target_slug) =
        if resolved_species == "_No_Bird_Detected" || resolved_species == "NoBirdDetected" {
            (RoutingTier::NoBirdDetected, "_No_Bird_Detected".to_string())
        } else if let Some(stripped) = resolved_species.strip_prefix("_Needs_Review/") {
            (RoutingTier::NeedsReview, sanitize_species_slug(stripped))
        } else {
            (
                RoutingTier::ConfirmedSpecies,
                sanitize_species_slug(&resolved_species),
            )
        };

    let target_dir = match routing_tier {
        RoutingTier::ConfirmedSpecies => operating_root.join(&target_slug),
        RoutingTier::NeedsReview => operating_root.join("_Needs_Review").join(&target_slug),
        RoutingTier::NoBirdDetected => operating_root.join("_No_Bird_Detected"),
    };

    if mode == OrganizationMode::HierarchicalHardlinkHierarchy && operating_root.exists() {
        fs::create_dir_all(&target_dir).with_context(|| {
            format!(
                "Failed to create target directory: {}",
                target_dir.display()
            )
        })?;
    }

    let mut results = Vec::with_capacity(bundles_to_update.len());
    let mut old_dirs_to_prune = std::collections::HashSet::new();

    let storage_root = input_root.unwrap_or(operating_root);
    let storage_backend = detect_storage_backend(storage_root);

    for (decision, bundle) in bundles_to_update {
        // Inspect locality prior to movement/rename so dataless stubs can be evicted post-rename
        let primary_locality = storage_backend.inspect_locality(&bundle.primary_image).ok();
        let was_dataless = primary_locality
            .as_ref()
            .map(|loc| loc.is_dataless || !loc.pre_existing_local)
            .unwrap_or(false);

        db.update_bundle_override(&bundle.bundle_id, &target_slug, routing_tier, 1.0)?;

        match mode {
            OrganizationMode::HierarchicalHardlinkHierarchy => {
                if operating_root.exists() {
                    let ops = db.get_bundle_operations(&bundle.bundle_id)?;
                    let mut moved_any = false;

                    for op in ops {
                        if op.operation_type == "hardlink" && op.status == "linked" {
                            let current_dest = &op.dest_path;
                            if let Some(fname) = current_dest.file_name() {
                                let new_dest = target_dir.join(fname);
                                if current_dest.exists() {
                                    if *current_dest != new_dest {
                                        if let Some(parent) = current_dest.parent() {
                                            old_dirs_to_prune.insert(parent.to_path_buf());
                                        }
                                        fs::rename(current_dest, &new_dest).with_context(|| {
                                            format!(
                                                "Failed to move hard link from '{}' to '{}'",
                                                current_dest.display(),
                                                new_dest.display()
                                            )
                                        })?;
                                        if was_dataless {
                                            let _ = storage_backend.evict(&new_dest);
                                        }
                                        db.update_operation_dest(op.op_id.unwrap_or(0), &new_dest)?;
                                        moved_any = true;
                                    }
                                } else if op.source_path.exists() && !new_dest.exists() {
                                    fs::hard_link(&op.source_path, &new_dest).with_context(
                                        || {
                                            format!(
                                                "Failed to hardlink from '{}' to '{}'",
                                                op.source_path.display(),
                                                new_dest.display()
                                            )
                                        },
                                    )?;
                                    if was_dataless {
                                        let _ = storage_backend.evict(&new_dest);
                                    }
                                    db.update_operation_dest(op.op_id.unwrap_or(0), &new_dest)?;
                                    moved_any = true;
                                }
                            }
                        }
                    }

                    if !moved_any {
                        let mut new_dec = decision.clone();
                        new_dec.assigned_species_slug = target_slug.clone();
                        new_dec.routing_tier = routing_tier;
                        new_dec.final_confidence = 1.0;
                        new_dec.is_user_override = true;

                        let latest_run = db
                            .get_latest_run_id()?
                            .unwrap_or_else(|| "run-manual".to_string());
                        let materialized = materialize_bundle_hardlinks(
                            &bundle,
                            &new_dec,
                            operating_root,
                            &latest_run,
                            false,
                        )?;
                        db.record_hardlink_operations(&materialized)?;
                    }
                }
            }

            OrganizationMode::InPlaceFileRenameToDescriptiveNames => {
                let ops = db.get_bundle_operations(&bundle.bundle_id)?;
                let mut current_primary = bundle.primary_image.clone();
                for op in &ops {
                    if op.operation_type == "in_place_rename"
                        && op.status == "renamed"
                        && op.dest_path.exists()
                    {
                        if let Some(src_fname) = op.source_path.file_name() {
                            if bundle.primary_image.file_name() == Some(src_fname) {
                                current_primary = op.dest_path.clone();
                            }
                        }
                    }
                }

                let mut current_bundle = bundle.clone();
                current_bundle.primary_image = current_primary;

                let mut new_comps = Vec::new();
                for comp in &bundle.companion_images {
                    let mut resolved = comp.clone();
                    for op in &ops {
                        if op.source_path == *comp && op.dest_path.exists() {
                            resolved = op.dest_path.clone();
                        }
                    }
                    new_comps.push(resolved);
                }
                current_bundle.companion_images = new_comps;

                let mut new_scs = Vec::new();
                for sc in &bundle.sidecar_files {
                    let mut resolved = sc.clone();
                    for op in &ops {
                        if op.source_path == *sc && op.dest_path.exists() {
                            resolved = op.dest_path.clone();
                        }
                    }
                    new_scs.push(resolved);
                }
                current_bundle.sidecar_files = new_scs;

                let date = bundle.capture_timestamp_ms.and_then(|ms| {
                    chrono::DateTime::from_timestamp_millis(ms)
                        .map(|dt| dt.format("%Y%m%d").to_string())
                });
                let loc = bundle.resolved_location_name.as_deref();

                let new_stem = format_inplace_filename(
                    &bundle.original_stem,
                    &target_slug,
                    date.as_deref(),
                    loc,
                    "",
                );

                let renames = execute_inplace_bundle_rename_with_stem(&current_bundle, &new_stem)?;
                let latest_run = db
                    .get_latest_run_id()?
                    .unwrap_or_else(|| "run-manual".to_string());
                for (old_path, new_path) in renames {
                    if was_dataless {
                        let _ = storage_backend.evict(&new_path);
                    }
                    db.record_inplace_rename(&latest_run, &old_path, &new_path)?;
                }
            }
        }

        let mut updated_dec = decision;
        updated_dec.assigned_species_slug = target_slug.clone();
        updated_dec.routing_tier = routing_tier;
        updated_dec.final_confidence = 1.0;
        updated_dec.is_user_override = true;
        results.push((updated_dec, bundle));
    }

    if mode == OrganizationMode::HierarchicalHardlinkHierarchy && operating_root.exists() {
        for old_dir in old_dirs_to_prune {
            if old_dir != operating_root && old_dir.exists() {
                let is_empty = fs::read_dir(&old_dir)
                    .map(|mut i| i.next().is_none())
                    .unwrap_or(false);
                if is_empty {
                    let _ = fs::remove_dir(&old_dir);
                }
            }
        }
    }

    // Regenerate bird_catalog.html if catalog or output tree exists
    let catalog_path = operating_root.join(crate::catalog::DEFAULT_CATALOG_FILENAME);
    if operating_root.exists() {
        let trip_name = input_root
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Avian Photos");

        let all_db_bundles = db.get_all_bundles(None)?;
        let mut catalog_items = Vec::with_capacity(all_db_bundles.len());

        for (dec, b) in all_db_bundles {
            let (prim_dest, comp_dests, sidecar_dests) = match mode {
                OrganizationMode::HierarchicalHardlinkHierarchy => {
                    let ops = db.get_bundle_operations(&b.bundle_id).unwrap_or_default();
                    let p_dest = ops
                        .iter()
                        .find(|op| {
                            op.source_path == b.primary_image && op.operation_type == "hardlink"
                        })
                        .map(|op| op.dest_path.clone())
                        .or_else(|| {
                            let expected = compute_target_dir(operating_root, &dec);
                            let dst_name = b.primary_image.file_name()?;
                            let p = expected.join(dst_name);
                            if p.exists() {
                                Some(p)
                            } else {
                                None
                            }
                        });
                    let c_dests: Vec<PathBuf> = b
                        .companion_images
                        .iter()
                        .filter_map(|c| {
                            ops.iter()
                                .find(|op| op.source_path == *c && op.operation_type == "hardlink")
                                .map(|op| op.dest_path.clone())
                        })
                        .collect();
                    let sc_dests: Vec<PathBuf> = b
                        .sidecar_files
                        .iter()
                        .filter_map(|sc| {
                            ops.iter()
                                .find(|op| op.source_path == *sc && op.operation_type == "hardlink")
                                .map(|op| op.dest_path.clone())
                        })
                        .collect();
                    (p_dest, Some(c_dests), Some(sc_dests))
                }
                OrganizationMode::InPlaceFileRenameToDescriptiveNames => (
                    Some(b.primary_image.clone()),
                    Some(b.companion_images.clone()),
                    Some(b.sidecar_files.clone()),
                ),
            };

            catalog_items.push(CatalogItem::from_bundle_and_decision(
                &b,
                &dec,
                prim_dest,
                comp_dests,
                sidecar_dests,
            ));
        }

        let _ = generate_html_catalog(trip_name, &catalog_items, mode, &catalog_path);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taxonomy_loading_has_curated_species() {
        let taxonomy = load_taxonomy_records();
        assert!(
            taxonomy.len() >= 30,
            "taxonomy should have curated fallback or models entries"
        );
        assert!(
            taxonomy.iter().any(|t| t.common_name == "Painted Stork"),
            "Painted Stork must be present in taxonomy"
        );
        assert!(
            taxonomy.iter().any(|t| t.common_name == "Mallard"),
            "Mallard must be present in taxonomy"
        );
    }

    #[test]
    fn test_exact_matches_return_no_suggestion() {
        let taxonomy = load_taxonomy_records();
        assert_eq!(
            find_species_typo_suggestion("Painted Stork", &taxonomy),
            None
        );
        assert_eq!(
            find_species_typo_suggestion("painted stork", &taxonomy),
            None
        );
        assert_eq!(
            find_species_typo_suggestion("Painted_Stork", &taxonomy),
            None
        );
        assert_eq!(
            find_species_typo_suggestion("painted_stork", &taxonomy),
            None
        );
        assert_eq!(find_species_typo_suggestion("Mallard", &taxonomy), None);
        assert_eq!(
            find_species_typo_suggestion("_No_Bird_Detected", &taxonomy),
            None
        );
        assert_eq!(
            find_species_typo_suggestion("NoBirdDetected", &taxonomy),
            None
        );
    }

    #[test]
    fn test_typo_suggestions_fuzzy_trigram() {
        let taxonomy = load_taxonomy_records();

        let s1 =
            find_species_typo_suggestion("Panted Stork", &taxonomy).expect("typo should match");
        assert_eq!(s1.common_name, "Painted Stork");
        assert_eq!(s1.folder_slug, "Painted_Stork");
        assert!(s1.similarity > 0.75);

        let s2 = find_species_typo_suggestion("Mallrd", &taxonomy).expect("typo should match");
        assert_eq!(s2.common_name, "Mallard");
        assert!(s2.similarity >= 0.60);

        let s3 = find_species_typo_suggestion("Panted_Stork", &taxonomy)
            .expect("slug typo should match");
        assert_eq!(s3.common_name, "Painted Stork");

        let s_none = find_species_typo_suggestion("UnrelatedCameraMakeModel123", &taxonomy);
        assert_eq!(s_none, None);
    }
}
