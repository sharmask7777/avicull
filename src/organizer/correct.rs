//! # Species Mislabel Curation, Interactive Correction & Batch Execution
//!
//! Provides CLI and programmatic interfaces to correct misclassified photo bundles:
//! - Applies a human curation override to a bundle, relocating hard links or renaming in-place assets, updating SQLite with `is_user_override = 1`.
//! - Recommends close eBird taxonomy matches using trigram similarity when typos are detected.
//! - Optionally propagates corrections across all images in the same temporal capture burst.

use crate::db::ManifestDb;
use crate::location::trigram_similarity;
use crate::organizer::inplace::{execute_inplace_bundle_rename_with_stem, format_inplace_filename};
use crate::organizer::{
    build_output_destination_path, materialize_bundle_hardlinks_with_category,
    sanitize_species_slug,
};
use crate::storage::detect_storage_backend;
use crate::types::{
    is_known_bird_slug, is_known_botanical_slug, AssetBundle, AssetCategory, BundleDecision,
    Classification, OrganizationMode, RoutingTier,
};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Authoritative record from eBird / AviList taxonomy for typo matching and suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyEntry {
    pub category: String,
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
    if std::env::var("AVICULL_NO_PROMPT").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        return false;
    }
    if cfg!(test)
        || std::env::current_exe().is_ok_and(|exe| {
            exe.file_name().is_some_and(|name| {
                let s = name.to_string_lossy();
                s.contains("test_suite") || s.starts_with("test")
            })
        })
    {
        return std::env::var("AVICULL_FORCE_INTERACTIVE")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    }
    (std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
        || std::env::var("AVICULL_FORCE_INTERACTIVE")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Loads eBird taxonomy records from cached or local models, falling back to the canonical embedded taxonomy.
pub fn load_taxonomy_records() -> Vec<TaxonomyEntry> {
    let mut candidate_paths = vec![
        PathBuf::from("models/taxonomy.json"),
        PathBuf::from("../models/taxonomy.json"),
        PathBuf::from("data/taxonomy.json"),
        PathBuf::from("../data/taxonomy.json"),
    ];

    if let Ok(cargo_manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        candidate_paths.push(
            PathBuf::from(&cargo_manifest)
                .join("data")
                .join("taxonomy.json"),
        );
        candidate_paths.push(
            PathBuf::from(&cargo_manifest)
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

    let mut content_opt = None;
    for path in candidate_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                content_opt = Some(content);
                break;
            }
        }
    }

    let raw_content: &str = match &content_opt {
        Some(s) => s.as_str(),
        None => crate::types::raw_taxonomy_json(),
    };

    if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(raw_content) {
        for obj in parsed {
            if let Some(common_name) = obj.get("common_name").and_then(|v| v.as_str()) {
                let category = obj
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Bird")
                    .to_string();
                let folder_slug = obj.get("folder_slug").and_then(|v| v.as_str()).map_or_else(
                    || sanitize_species_slug(common_name),
                    std::string::ToString::to_string,
                );
                let scientific_name = obj
                    .get("scientific_name")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string);
                if seen.insert(common_name.to_lowercase()) {
                    entries.push(TaxonomyEntry {
                        category,
                        common_name: common_name.to_string(),
                        folder_slug,
                        scientific_name,
                    });
                }
            }
        }
    }

    entries
}

/// Identifies whether an input string is a probable typo for an authoritative taxonomy species
/// using character trigram similarity. Returns None if the input is already recognized as an
/// exact common name, folder slug, or scientific name.
pub fn find_species_typo_suggestion(
    input: &str,
    taxonomy: &[TaxonomyEntry],
) -> Option<SpeciesTypoSuggestion> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || trimmed == RoutingTier::SENTINEL_NO_BIRD_DETECTED
        || trimmed == RoutingTier::SENTINEL_NO_SUBJECT_DETECTED
        || trimmed == "NoBirdDetected"
        || trimmed == "NoSubjectDetected"
        || trimmed.eq_ignore_ascii_case("No Bird Detected")
        || trimmed.eq_ignore_ascii_case("No_Bird_Detected")
        || trimmed.eq_ignore_ascii_case("No Subject Detected")
        || trimmed.eq_ignore_ascii_case("No_Subject_Detected")
    {
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
                .is_some_and(|s| s.eq_ignore_ascii_case(trimmed))
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
            .map_or(0.0, |s| trigram_similarity(trimmed, s));

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

/// Applies a direct manual classification correction for a single photo bundle or burst defaulting to `"Bird"`.
pub fn apply_bundle_correction(
    db: &ManifestDb,
    operating_root: &Path,
    mode: OrganizationMode,
    file_query: &str,
    new_species: &str,
    input_root: Option<&Path>,
    propagate_burst: bool,
) -> Result<Vec<(BundleDecision, AssetBundle)>> {
    apply_bundle_correction_with_category(
        db,
        operating_root,
        mode,
        file_query,
        "Bird",
        new_species,
        input_root,
        propagate_burst,
    )
}

/// Applies a direct manual classification correction for a single photo bundle or burst with an explicit category.
///
/// Updates the bundle's classification, category, and routing tier in the SQLite manifest,
/// marks it as a durable user override (`is_user_override = 1`, `final_confidence = 1.0`),
/// moves or renames all associated files (primary image, companions, and sidecars)
/// according to the active organization mode, and regenerates `catalog.html`.
/// When `propagate_burst` is enabled or confirmed interactively, all frames belonging
/// to the same burst are reclassified and relocated lockstep.
#[allow(clippy::too_many_arguments)]
pub fn apply_bundle_correction_with_category(
    db: &ManifestDb,
    operating_root: &Path,
    mode: OrganizationMode,
    file_query: &str,
    category: &str,
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
    if query_trim.is_empty() || query_trim == "./" || query_trim == ".\\" {
        return Err(anyhow!(
            "Photo query cannot be empty. Please specify an unambiguous file path or stem."
        ));
    }
    let mut matched_bundles = db.find_candidate_bundle_entries_by_query(file_query)?;

    let (target_dec, target_bundle) = match matched_bundles.len() {
        0 => {
            return Err(anyhow!(
                "No photo bundle matching '{file_query}' was found in manifest."
            ));
        }
        1 => matched_bundles.remove(0),
        _ => {
            let is_interactive = is_interactive_session();
            if is_interactive {
                println!("\nMultiple photo bundles matched query '{file_query}':");
                for (i, (dec, b)) in matched_bundles.iter().enumerate() {
                    let rel = b.relative_path.as_ref().map_or_else(
                        || b.primary_image.to_string_lossy().to_string(),
                        |p| p.to_string_lossy().to_string(),
                    );
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
                        dec.assigned_species_slug()
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
                use std::fmt::Write;
                let mut msg = format!("Multiple photo bundles matched query '{file_query}':\n");
                for (_dec, b) in &matched_bundles {
                    let rel = b.relative_path.as_ref().map_or_else(
                        || b.primary_image.to_string_lossy().to_string(),
                        |p| p.to_string_lossy().to_string(),
                    );
                    let _ = writeln!(msg, "  - {rel}");
                }
                msg.push_str("Specify an unambiguous relative path using --file (e.g. --file \"<trip>/<file>\").");
                return Err(anyhow!(msg));
            }
        }
    };

    // Identify burst members if the bundle is part of a multi-frame burst
    let burst_members: Vec<(BundleDecision, AssetBundle)> =
        if let Some(ref b_id) = target_dec.burst_id {
            let all_bundles = db.get_all_bundles(None)?;
            all_bundles
                .into_iter()
                .filter(|(d, b)| {
                    d.burst_id.as_deref() == Some(b_id)
                        && b.trip_folder_name == target_bundle.trip_folder_name
                })
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

    let mut normalized_category: String = match category.to_ascii_lowercase().as_str() {
        "mammalia" | "mammal" | "mammals" => "Mammalia".to_string(),
        "insecta" | "insect" | "insects" => "Insecta".to_string(),
        "reptilia" | "reptile" | "reptiles" => "Reptilia".to_string(),
        "amphibia" | "amphibian" | "amphibians" => "Amphibia".to_string(),
        "scenery" | "landscape" => "Scenery".to_string(),
        "architecture" | "monument" | "building" => "Architecture".to_string(),
        "portrait" | "people" => "Portrait".to_string(),
        "abstract" | "texture" => "Abstract".to_string(),
        "botanical"
        | "plant"
        | "plants"
        | "flora"
        | "flower"
        | "flowers"
        | "tree"
        | "trees"
        | "no_subject_detected"
        | "nosubjectdetected"
        | "no_bird_detected"
        | "nobirddetected" => AssetCategory::NoSubjectDetected.as_str().to_string(),
        "bird" | "birds" => "Bird".to_string(),
        other => {
            if other.is_empty() {
                "Bird".to_string()
            } else {
                category.to_string()
            }
        }
    };

    let mut resolved_species = new_species.to_string();
    let (has_needs_review, mut species_to_check) =
        if let Some(stripped) = new_species.strip_prefix(RoutingTier::PREFIX_NEEDS_REVIEW) {
            (true, stripped)
        } else {
            (false, new_species)
        };

    let mut is_no_bird = species_to_check == RoutingTier::SENTINEL_NO_BIRD_DETECTED
        || species_to_check == "NoBirdDetected"
        || species_to_check.eq_ignore_ascii_case("No Bird Detected")
        || species_to_check.eq_ignore_ascii_case("No_Bird_Detected")
        || species_to_check == RoutingTier::SENTINEL_NO_SUBJECT_DETECTED
        || species_to_check == "NoSubjectDetected"
        || species_to_check.eq_ignore_ascii_case("No Subject Detected")
        || species_to_check.eq_ignore_ascii_case("No_Subject_Detected");

    let mut suggestion_declined = false;
    let mut has_bird_suggestion = false;
    if normalized_category == "Bird" && !is_no_bird && !species_to_check.is_empty() {
        let taxonomy = load_taxonomy_records();
        if let Some(suggestion) = find_species_typo_suggestion(species_to_check, &taxonomy) {
            has_bird_suggestion = true;
            if is_interactive_session() {
                print!(
                    "\nSpecies '{}' not found in taxonomy. Did you mean '{}'? [Y/n]: ",
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
                } else {
                    suggestion_declined = true;
                }
            } else {
                println!(
                    "[avicull] Note: Species '{}' not found in taxonomy. Did you mean '{}'?",
                    species_to_check, suggestion.common_name
                );
            }
        }
    }

    if let Some(stripped) = resolved_species.strip_prefix(RoutingTier::PREFIX_NEEDS_REVIEW) {
        species_to_check = stripped;
    } else {
        species_to_check = &resolved_species;
    }

    if normalized_category == "Bird" {
        if is_no_bird {
            normalized_category = AssetCategory::NoSubjectDetected.as_str().to_string();
        } else if species_to_check.trim().is_empty() {
            return Err(anyhow!(
                "Species name cannot be empty for category Bird. If no bird is detected, use --category No_Subject_Detected or --species _No_Bird_Detected"
            ));
        } else if !is_known_bird_slug(species_to_check) {
            let cat = AssetCategory::from_slug(species_to_check);
            if cat != AssetCategory::Bird && cat != AssetCategory::NoSubjectDetected {
                normalized_category = cat.as_str().to_string();
            } else if is_known_botanical_slug(species_to_check) {
                normalized_category = AssetCategory::NoSubjectDetected.as_str().to_string();
                is_no_bird = true;
            } else if suggestion_declined || !has_bird_suggestion {
                // When explicitly categorized as Bird with a novel custom species or hybrid
                // that is not in the standard taxonomy, respect the user's category selection.
                normalized_category = "Bird".to_string();
            }
        }
    } else if normalized_category == AssetCategory::NoSubjectDetected.as_str() {
        is_no_bird = true;
    }

    let (routing_tier, target_slug) = if is_no_bird {
        if species_to_check.to_ascii_lowercase().contains("bird") {
            (
                RoutingTier::NoBirdDetected,
                RoutingTier::SENTINEL_NO_BIRD_DETECTED.to_string(),
            )
        } else {
            (
                RoutingTier::NoBirdDetected,
                RoutingTier::SENTINEL_NO_SUBJECT_DETECTED.to_string(),
            )
        }
    } else if let Some(stripped) = resolved_species.strip_prefix(RoutingTier::PREFIX_NEEDS_REVIEW) {
        (RoutingTier::NeedsReview, sanitize_species_slug(stripped))
    } else if species_to_check.trim().is_empty() {
        (RoutingTier::ConfirmedSpecies, String::new())
    } else {
        (
            RoutingTier::ConfirmedSpecies,
            sanitize_species_slug(&resolved_species),
        )
    };

    let target_dir = build_output_destination_path(
        operating_root,
        &normalized_category,
        &target_slug,
        routing_tier,
    );

    if mode == OrganizationMode::HierarchicalHardlinkHierarchy && operating_root.exists() {
        fs::create_dir_all(&target_dir).with_context(|| {
            format!(
                "Failed to create target directory: {}",
                target_dir.display()
            )
        })?;
        if routing_tier == RoutingTier::NoBirdDetected {
            let legacy_symlink = operating_root.join(RoutingTier::DIR_NO_BIRD_DETECTED);
            if !legacy_symlink.exists() && fs::symlink_metadata(&legacy_symlink).is_err() {
                let _ = std::os::unix::fs::symlink(
                    RoutingTier::DIR_NO_SUBJECT_DETECTED,
                    &legacy_symlink,
                );
            }
        }
    }

    let mut results = Vec::with_capacity(bundles_to_update.len());
    let mut old_dirs_to_prune = std::collections::HashSet::new();

    let storage_root = input_root.unwrap_or(operating_root);
    let storage_backend = detect_storage_backend(storage_root);

    for (mut decision, bundle) in bundles_to_update {
        // Inspect locality prior to movement/rename so dataless stubs can be evicted post-rename
        let primary_locality = storage_backend.inspect_locality(&bundle.primary_image).ok();
        let was_dataless = primary_locality
            .as_ref()
            .is_some_and(|loc| loc.is_dataless || !loc.pre_existing_local);

        let norm_cat: AssetCategory = normalized_category
            .parse()
            .unwrap_or(AssetCategory::NoSubjectDetected);
        decision.classification = if target_slug.is_empty() {
            Classification::top_level(norm_cat)
        } else {
            Classification::granular(norm_cat, &target_slug)
        };
        decision.routing_tier = routing_tier;
        decision.final_confidence = 1.0;
        decision.is_user_override = true;

        db.update_bundle_override_with_category(
            &bundle.bundle_id,
            &normalized_category,
            &target_slug,
            routing_tier,
            1.0,
        )?;

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
                        let latest_run = db
                            .get_latest_run_id()?
                            .unwrap_or_else(|| "run-manual".to_string());
                        let materialized = materialize_bundle_hardlinks_with_category(
                            &bundle,
                            &decision,
                            &normalized_category,
                            operating_root,
                            &latest_run,
                            false,
                        )?;
                        db.record_hardlink_operations(&materialized)?;
                    }
                }
            }

            OrganizationMode::InPlace => {
                // Untouched files on disk: only SQLite records are updated with user override
            }

            OrganizationMode::InPlaceRename => {
                let ops = db.get_bundle_operations(&bundle.bundle_id)?;
                let mut current_primary = bundle.primary_image.clone();
                for op in &ops {
                    if op.operation_type == "in_place_rename"
                        && op.status == "renamed"
                        && op.dest_path.exists()
                    {
                        if let Some(src_fname) = op.source_path.file_name() {
                            if bundle.primary_image.file_name() == Some(src_fname) {
                                current_primary.clone_from(&op.dest_path);
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
                            resolved.clone_from(&op.dest_path);
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
                            resolved.clone_from(&op.dest_path);
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
                    &normalized_category,
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

        results.push((decision, bundle));
    }

    if mode == OrganizationMode::HierarchicalHardlinkHierarchy && operating_root.exists() {
        for old_dir in old_dirs_to_prune {
            if old_dir != operating_root && old_dir.exists() {
                let is_empty = fs::read_dir(&old_dir).is_ok_and(|mut i| i.next().is_none());
                if is_empty {
                    let _ = fs::remove_dir(&old_dir);
                }
                if let Some(parent) = old_dir.parent() {
                    if parent != operating_root && parent.exists() {
                        let is_parent_empty =
                            fs::read_dir(parent).is_ok_and(|mut i| i.next().is_none());
                        if is_parent_empty {
                            let _ = fs::remove_dir(parent);
                        }
                    }
                }
            }
        }
    }

    // Regenerate catalog.html if catalog or output tree exists
    if operating_root.exists() {
        let trip_name = input_root
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        let _ = crate::organizer::regenerate_catalog_from_manifest(
            db,
            operating_root,
            trip_name,
            Some(mode),
        );
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
