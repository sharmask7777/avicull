#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

use anyhow::{anyhow, Result};
use avicull::cli::{Cli, Commands, CorrectArgs, LocationArgs};
use avicull::db::ManifestDb;
use avicull::geo::resolve_locations;
use avicull::location::gazetteer::GazetteerEngine;
use avicull::organizer::{clean_run_hardlinks, clean_run_operations, materialize_bundle_hardlinks};
use avicull::scanner::{scan_input_tree_with_skip, validate_volume_and_disjointness};
use avicull::types::{OrganizationMode, RoutingTier};
use clap::Parser;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Perform periodic background update check on normal CLI invocations
    if !matches!(cli.command, Commands::Update(_)) {
        if let Some(notice) = avicull::update::check_for_update_notice(env!("CARGO_PKG_VERSION")) {
            eprintln!("{notice}\n");
        }
    }

    match cli.command {
        Commands::Scan(args) => {
            println!("== avicull scan ==");
            let output_dir = args
                .output
                .unwrap_or_else(|| args.input.join("birds_hierarchy"));
            validate_volume_and_disjointness(&args.input, &output_dir)?;
            println!("✓ APFS same-volume and path validation succeeded.");

            println!("Scanning source directory '{}'...", args.input.display());
            let mut bundles = scan_input_tree_with_skip(&args.input, Some(&output_dir))?;
            println!("✓ Discovered {} asset bundles.", bundles.len());

            println!(
                "Resolving EXIF metadata and GPS coordinates across {} bundles...",
                bundles.len()
            );
            let def_coords = match (args.default_lat, args.default_lon) {
                (Some(la), Some(lo)) => Some((la, lo)),
                _ => None,
            };
            resolve_locations(&mut bundles, args.locations_file.as_deref(), def_coords)?;

            let with_gps = bundles.iter().filter(|b| b.gps_coords.is_some()).count();
            println!(
                "✓ Resolved GPS coordinates for {}/{} bundles.",
                with_gps,
                bundles.len()
            );

            let total_sidecars: usize = bundles.iter().map(|b| b.sidecar_files.len()).sum();
            let total_companions: usize = bundles.iter().map(|b| b.companion_images.len()).sum();
            println!(
                "✓ Associated {total_companions} companion images and {total_sidecars} sidecar files."
            );
            println!("Target output: {}", output_dir.display());
            println!("Ready for organize.");
        }

        Commands::Organize(args) => {
            avicull::organizer::run_organize(&args)?;
        }

        Commands::Relink(args) => {
            println!("== avicull relink ==");
            let output_dir = args
                .output
                .clone()
                .or_else(|| args.input.as_ref().map(|p| p.join("birds_hierarchy")))
                .unwrap_or_else(|| PathBuf::from("birds_hierarchy"));
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if args.output.is_some() {
                output_dir.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                output_dir.join(".avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db.get_latest_run_id()?.ok_or_else(|| {
                    anyhow!(
                        "No existing run found in manifest.sqlite at '{}'",
                        db_path.display()
                    )
                })?,
            };

            println!(
                "Relinking run '{}' with min-confidence: {:.2}...",
                run_id, args.min_confidence
            );
            let cached_bundles = db.get_all_bundles(Some(&run_id))?;
            println!("Loaded {} cached bundle predictions.", cached_bundles.len());

            // Clean previous links
            let unlinked = clean_run_hardlinks(&db, &run_id)?;
            println!("Unlinked {unlinked} previous hard links.");

            let total_relink = cached_bundles.len();
            println!(
                "Materializing hard links for {total_relink} bundles into '{}'...",
                output_dir.display()
            );
            let mut new_ops = Vec::new();
            for (idx, (mut dec, bundle)) in cached_bundles.into_iter().enumerate() {
                if dec.final_confidence >= args.min_confidence {
                    dec.routing_tier = RoutingTier::ConfirmedSpecies;
                } else if dec.detection_confidence >= 0.25 {
                    dec.routing_tier = RoutingTier::NeedsReview;
                } else {
                    dec.routing_tier = RoutingTier::NoBirdDetected;
                }

                db.update_bundle_routing_tier(&bundle.bundle_id, dec.routing_tier)?;
                let ops = materialize_bundle_hardlinks(&bundle, &dec, &output_dir, &run_id, false)?;
                new_ops.extend(ops);
                if args.verbose || (idx + 1).is_multiple_of(50) || (idx + 1) == total_relink {
                    println!(
                        "  Relinked {}/{} bundles ({} links created)...",
                        idx + 1,
                        total_relink,
                        new_ops.len()
                    );
                }
            }

            db.record_hardlink_operations(&new_ops)?;
            println!("✓ Re-materialized {} hard links instantly.", new_ops.len());
        }

        Commands::Clean(args) => {
            println!("== avicull clean ==");
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if let Some(ref out) = args.output {
                out.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => id,
                None => db.get_latest_run_id()?.ok_or_else(|| {
                    anyhow!(
                        "No existing run found in manifest.sqlite at '{}'",
                        db_path.display()
                    )
                })?,
            };

            println!("Cleaning operations for run '{run_id}'...");
            let count = clean_run_operations(&db, &run_id)?;
            println!("✓ Cleaned {count} operations for run '{run_id}'.");
        }

        Commands::Report(args) => {
            let db_path = if let Some(ref db_p) = args.db {
                db_p.clone()
            } else if let Some(ref out) = args.output {
                out.join(".avicull/manifest.sqlite")
            } else if let Some(ref inp) = args.input {
                let inplace_db = inp.join(".avicull/manifest.sqlite");
                let hier_db = inp.join("birds_hierarchy/.avicull/manifest.sqlite");
                if inplace_db.exists() {
                    inplace_db
                } else if hier_db.exists() {
                    hier_db
                } else {
                    inplace_db
                }
            } else {
                PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite")
            };
            let db = ManifestDb::open_or_create(&db_path)?;

            let run_id = match args.run_id {
                Some(id) => Some(id),
                None => db.get_latest_run_id()?,
            };

            let cached = db.get_all_bundles(run_id.as_deref())?;

            let fmt = args.format.to_lowercase();
            if fmt == "csv" {
                println!("bundle_id,trip_folder,original_stem,species,final_conf,routing_tier");
                for (dec, b) in cached {
                    println!(
                        "{},{},{},{},{:.2},{:?}",
                        b.bundle_id,
                        b.trip_folder_name,
                        b.original_stem,
                        dec.assigned_species_slug,
                        dec.final_confidence,
                        dec.routing_tier
                    );
                }
            } else if fmt == "table" {
                use std::collections::BTreeMap;
                let mut species_stats: BTreeMap<String, (usize, f32)> = BTreeMap::new();
                for (dec, _) in &cached {
                    let entry = species_stats
                        .entry(dec.assigned_species_slug.clone())
                        .or_insert((0, 0.0));
                    entry.0 += 1;
                    entry.1 += dec.final_confidence;
                }
                println!(
                    "{:<35} | {:<8} | {:<10}",
                    "Species Slug", "Count", "Mean Conf"
                );
                println!("{:-<35}-+-{:-<8}-+-{:-<10}", "", "", "");
                for (species, (count, total_conf)) in species_stats {
                    let mean_conf = if count > 0 {
                        total_conf / count as f32
                    } else {
                        0.0
                    };
                    println!("{species:<35} | {count:<8} | {mean_conf:<10.2}");
                }
                println!("\nTotal Bundles: {}", cached.len());
            } else {
                let records: Vec<_> = cached
                    .into_iter()
                    .map(|(dec, b)| {
                        serde_json::json!({
                            "bundle_id": b.bundle_id,
                            "trip_folder": b.trip_folder_name,
                            "original_stem": b.original_stem,
                            "assigned_species_slug": dec.assigned_species_slug,
                            "final_confidence": dec.final_confidence,
                            "routing_tier": format!("{:?}", dec.routing_tier),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&records)?);
            }
        }

        Commands::Models(args) => {
            let home = std::env::var("HOME").ok();
            let default_cache = home.map(|h| PathBuf::from(h).join(".cache/avicull/models"));
            let cache_dir = args
                .cache_dir
                .or(default_cache)
                .unwrap_or_else(|| PathBuf::from("models"));

            println!("== avicull models ==");
            println!("Avian Model & Taxonomy Cache: '{}'", cache_dir.display());

            let tax_path = cache_dir.join("taxonomy.json");
            let emb_path = cache_dir.join("species_embeddings.safetensors");
            let manifest_path = cache_dir.join("manifest.json");

            if args.action == "update" {
                println!(
                    "Fetching latest Cornell eBird global taxonomy and recomputing embeddings..."
                );
                std::fs::create_dir_all(&cache_dir)?;
                let cache_dir_str = cache_dir.to_string_lossy();
                let status = std::process::Command::new("uv")
                    .args([
                        "run",
                        "--with",
                        "torch,open-clip-torch,safetensors,ultralytics",
                        "python3",
                        "scripts/build_taxonomy_embeddings.py",
                        cache_dir_str.as_ref(),
                    ])
                    .status()?;
                if status.success() {
                    println!(
                        "✓ Successfully updated model and taxonomy cache in '{}'",
                        cache_dir.display()
                    );
                } else {
                    return Err(anyhow!("Failed to update models"));
                }
            } else {
                let tax_status = if tax_path.exists() {
                    let sz = std::fs::metadata(&tax_path).map_or(0, |m| m.len());
                    format!("Present ({:.2} MB)", sz as f64 / 1_048_576.0)
                } else {
                    "Missing (run 'avicull models update' to install)".to_string()
                };

                let emb_status = if emb_path.exists() {
                    let sz = std::fs::metadata(&emb_path).map_or(0, |m| m.len());
                    format!("Present ({:.2} MB)", sz as f64 / 1_048_576.0)
                } else {
                    "Missing (run 'avicull models update' to install)".to_string()
                };

                println!("  - Taxonomy (taxonomy.json):             {tax_status}");
                println!("  - Embeddings (species_embeddings.safetensors): {emb_status}");

                if let Ok(meta) = std::fs::metadata(&manifest_path) {
                    if let Ok(mod_time) = meta.modified() {
                        if let Ok(dur) = mod_time.elapsed() {
                            let days = dur.as_secs() / 86400;
                            println!("  - Last Updated:                         {days} day(s) ago");
                        }
                    }
                }
            }
        }

        Commands::Update(args) => {
            avicull::update::perform_self_update(args.check)?;
        }

        Commands::Correct(args) => {
            execute_correct(&args)?;
        }

        Commands::Location(args) => {
            execute_location(args)?;
        }

        Commands::Help(args) => {
            let topic = match args.topic.as_deref() {
                Some(t) => Some(t.parse::<avicull::help::HelpTopic>()?),
                None => None,
            };
            print!("{}", avicull::help::render_help(topic));
        }

        Commands::CreateMockDataset(args) => {
            execute_create_mock_dataset(args)?;
        }

        Commands::Mount(args) => {
            execute_mount(&args);
        }

        Commands::Unmount(args) => {
            execute_unmount(&args);
        }

        Commands::OrganizeAll(_args) => {
            anyhow::bail!("organize-all implementation in progress");
        }
    }

    Ok(())
}

fn execute_mount(args: &avicull::cli::MountArgs) {
    let mount_point = args
        .mount_point
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("/Volumes/avicull_birds"));

    println!("== avicull mount ==");
    println!(
        "Source: {}",
        avicull::storage::opendal_backend::OpenDalBackend::sanitize_uri(&args.source)
    );
    println!("Mount point: {}", mount_point.display());

    // Build dedicated 2-worker Tokio runtime to protect Apple Silicon cores for Rayon/Metal
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("avicull-webdav")
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to initialize Tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    rt.block_on(async {
        // Resolve backend
        let backend = if args.source.starts_with("s3://") {
            let bucket = args.source.trim_start_matches("s3://");
            let bucket_name = bucket.split('/').next().unwrap_or(bucket);
            match avicull::storage::opendal_backend::OpenDalBackend::from_s3(
                bucket_name,
                args.endpoint.as_deref(),
                args.region.as_deref(),
            ) {
                Ok(b) => std::sync::Arc::new(b),
                Err(e) => {
                    eprintln!("Failed to connect to S3 source: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            let path = std::path::Path::new(&args.source);
            match avicull::storage::opendal_backend::OpenDalBackend::from_fs_path(path) {
                Ok(b) => std::sync::Arc::new(b),
                Err(e) => {
                    eprintln!("Failed to access local source directory: {e}");
                    std::process::exit(1);
                }
            }
        };

        let tree = std::sync::Arc::new(avicull::storage::webdav_server::TaxonomicTree::new());
        let auth_token = avicull::storage::mount_service::generate_auth_token();

        let config = avicull::storage::mount_service::MountServiceConfig {
            port: args.port,
            mount_point: mount_point.clone(),
            serve_only: args.serve_only,
            auth_token: auth_token.clone(),
        };

        let service = avicull::storage::mount_service::MountService::new(
            config,
            tree,
            backend,
        );

        let bound_addr = match service.bind_listener().await {
            Ok(addr) => addr,
            Err(e) => {
                eprintln!("Failed to bind loopback TCP port: {e}");
                std::process::exit(1);
            }
        };

        println!("WebDAV loopback server listening at: http://127.0.0.1:{}", bound_addr.port());

        #[cfg(target_os = "macos")]
        if !args.serve_only {
            match avicull::storage::mount_service::MountService::<avicull::storage::opendal_backend::OpenDalBackend>::mount_darwin(
                bound_addr.port(),
                &mount_point,
                &auth_token,
            ) {
                Ok(()) => {
                    println!("Successfully mounted virtual volume at {}", mount_point.display());
                    if args.open {
                        let _ = std::process::Command::new("open").arg(&mount_point).status();
                    }
                }
                Err(e) => {
                    eprintln!("Notice: OS mount_webdav failed: {e}. You can connect manually via Finder: Cmd+K -> http://127.0.0.1:{}", bound_addr.port());
                }
            }
        }

        println!("Virtual volume is active. Press Ctrl+C to unmount and exit.");

        // Clean teardown signal trap
        let _ = tokio::signal::ctrl_c().await;
        println!("\nReceived shutdown signal. Cleaning up mount point...");

        #[cfg(target_os = "macos")]
        if !args.serve_only {
            let _ = avicull::storage::mount_service::MountService::<avicull::storage::opendal_backend::OpenDalBackend>::unmount_darwin(&mount_point);
        }

        println!("Shutdown complete.");
    });
}

fn execute_unmount(args: &avicull::cli::UnmountArgs) {
    #[cfg(target_os = "macos")]
    {
        println!(
            "Unmounting virtual volume at: {}",
            args.mount_point.display()
        );
        match avicull::storage::mount_service::MountService::<
            avicull::storage::opendal_backend::OpenDalBackend,
        >::unmount_darwin(&args.mount_point)
        {
            Ok(()) => {
                println!("Successfully unmounted {}", args.mount_point.display());
            }
            Err(e) => {
                eprintln!("Failed to unmount volume: {e}");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("Unmount utility requires macOS diskutil.");
    }
}

/// Resolves the archive operating root directory hosting the `.avicull/manifest.sqlite` database.
///
/// If `db_path` is `<root>/.avicull/manifest.sqlite` (or `.avicull/manifest.sqlite`), strips the `.avicull`
/// directory component to return `<root>` (or `.` if relative to current directory).
/// This prevents generating catalogs inside `<input>/.avicull/catalog.html` or creating nested
/// `.avicull/.avicull/` preview directories.
fn resolve_operating_root_for_manifest(db_path: &Path) -> PathBuf {
    if let Some(parent) = db_path.parent() {
        if parent.file_name().is_some_and(|name| name == ".avicull") {
            return parent
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        }
        if !parent.as_os_str().is_empty() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(".")
}

/// Resolves the SQLite manifest database path with mode-aware directory precedence.
///
/// Prioritizes explicit `--db` or `--output` flags when provided. Otherwise, checks both
/// in-place (`<input>/.avicull/manifest.sqlite`) and hierarchy (`<input>/birds_hierarchy/.avicull/manifest.sqlite`)
/// databases, preferring `inplace_db` for default/in-place mode or `hier_db` for hierarchy mode.
fn resolve_manifest_db_path(
    db_arg: Option<&Path>,
    output_arg: Option<&Path>,
    input_arg: Option<&Path>,
    mode_arg: Option<OrganizationMode>,
) -> PathBuf {
    if let Some(db_p) = db_arg {
        return db_p.to_path_buf();
    }
    if let Some(out) = output_arg {
        return out.join(".avicull/manifest.sqlite");
    }
    let (hier_db, inplace_db) = if let Some(inp) = input_arg {
        (
            inp.join("birds_hierarchy/.avicull/manifest.sqlite"),
            inp.join(".avicull/manifest.sqlite"),
        )
    } else {
        (
            PathBuf::from("birds_hierarchy/.avicull/manifest.sqlite"),
            PathBuf::from(".avicull/manifest.sqlite"),
        )
    };

    match mode_arg {
        Some(OrganizationMode::HierarchicalHardlinkHierarchy) => {
            if hier_db.exists() {
                hier_db
            } else {
                inplace_db
            }
        }
        Some(OrganizationMode::InPlace | OrganizationMode::InPlaceRename) | None => {
            if inplace_db.exists() {
                inplace_db
            } else {
                hier_db
            }
        }
    }
}

fn execute_correct(args: &CorrectArgs) -> Result<()> {
    println!("== avicull correct ==");

    let db_path = resolve_manifest_db_path(
        args.db.as_deref(),
        args.output.as_deref(),
        args.input.as_deref(),
        args.mode,
    );

    if !db_path.exists() {
        return Err(anyhow!(
            "Manifest database not found at '{}'. Run 'avicull organize' first.",
            db_path.display()
        ));
    }

    let db = ManifestDb::open_or_create(&db_path)?;

    let mode = if let Some(m) = args.mode {
        m
    } else if let Ok(Some(m)) = db.get_latest_run_mode() {
        m
    } else if args.output.is_some() || db_path.to_string_lossy().contains("birds_hierarchy") {
        OrganizationMode::HierarchicalHardlinkHierarchy
    } else {
        OrganizationMode::InPlace
    };

    let operating_root = match mode {
        OrganizationMode::HierarchicalHardlinkHierarchy => args
            .output
            .clone()
            .or_else(|| args.input.as_ref().map(|p| p.join("birds_hierarchy")))
            .unwrap_or_else(|| resolve_operating_root_for_manifest(&db_path)),
        OrganizationMode::InPlace | OrganizationMode::InPlaceRename => args
            .input
            .clone()
            .unwrap_or_else(|| resolve_operating_root_for_manifest(&db_path)),
    };

    let results = avicull::organizer::apply_bundle_correction_with_category(
        &db,
        &operating_root,
        mode,
        &args.file,
        &args.category,
        &args.species,
        args.input.as_deref(),
        args.propagate_burst,
    )?;

    for (updated_dec, bundle) in &results {
        println!(
            "✓ Reclassified '{}' to '{}' (routing_tier: {:?}, is_user_override: 1).",
            bundle.original_stem, updated_dec.assigned_species_slug, updated_dec.routing_tier
        );
    }

    Ok(())
}

fn execute_location(args: LocationArgs) -> Result<()> {
    println!("== avicull location ==");

    let db_path = resolve_manifest_db_path(
        args.db.as_deref(),
        args.output.as_deref(),
        args.input.as_deref(),
        None,
    );

    if !db_path.exists() {
        return Err(anyhow!(
            "Manifest database not found at '{}'. Run 'avicull organize' first.",
            db_path.display()
        ));
    }

    let db = ManifestDb::open_or_create(&db_path)?;
    let engine = GazetteerEngine::load_embedded()?;

    let resolved = avicull::location::resolver::resolve_from_override(&args.location, &engine)
        .ok_or_else(|| {
            anyhow!(
                "Could not resolve location '{}'. Specify 'lat, lon' (e.g. '34.1526, 77.5771') or a known place name.",
                args.location
            )
        })?;

    if let Some(ref file_target) = args.file {
        // Resolve target bundle by stem, filename, or bundle_id
        let target_bundle = db
            .find_bundle_by_query(file_target)?
            .ok_or_else(|| anyhow!("Photo bundle '{file_target}' not found in database"))?;

        db.set_bundle_location(&target_bundle.bundle_id, &resolved)?;
        println!(
            "✓ Set photo '{}' ({}) location to '{}' (region: '{}')",
            target_bundle.original_stem,
            target_bundle.bundle_id,
            resolved.name,
            resolved.region_code
        );
    } else {
        // Apply to directory or subdirectory
        let dir_key = args.dir.unwrap_or_else(|| ".".to_string());
        db.set_directory_location(&dir_key, &resolved)?;

        println!(
            "✓ Set directory '{}' location to '{}' (region: '{}')",
            dir_key, resolved.name, resolved.region_code
        );
    }

    let operating_root = resolve_operating_root_for_manifest(&db_path);
    if operating_root.exists() {
        if let Ok(catalog_path) =
            avicull::organizer::regenerate_catalog_from_manifest(&db, &operating_root, None, None)
        {
            println!(
                "✓ Regenerated photo catalog with updated locations at '{}'",
                catalog_path.display()
            );
        }
    }

    Ok(())
}

fn execute_create_mock_dataset(args: avicull::cli::CreateMockDatasetArgs) -> Result<()> {
    let target_dir = args.target.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/local_user_samples")
    });

    println!(
        "Creating minimal local test dataset in '{}' from '{}'...",
        target_dir.display(),
        args.source.display()
    );

    let options = avicull::dataset::MiniDatasetOptions {
        source_dir: args.source,
        target_dir,
        force: args.force,
        curated_samples: avicull::dataset::default_curated_samples(),
        gazetteers: avicull::dataset::default_gazetteers(),
    };

    let result = avicull::dataset::create_mini_dataset(&options)?;

    println!(
        "\nSuccessfully created mini dataset with {} files ({:.2} MB).",
        result.copied_files_count,
        result.total_bytes as f64 / 1_048_576.0
    );

    Ok(())
}
