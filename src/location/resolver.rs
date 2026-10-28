// src/location/resolver.rs

use crate::location::gazetteer::GazetteerEngine;
use crate::manifest::ManifestDb;
use crate::types::{AssetBundle, GpsCoordinate, LocationSource};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, Write};

/// Applies strict four-tier location precedence to an AssetBundle:
/// 1. Embedded EXIF GPS (hardware camera coordinates are strictly immutable)
/// 2. Image-level explicit location override (bundle_locations table)
/// 3. Subdirectory / parent hierarchical location override (directory_locations table)
/// 4. Default / existing gazetteer fallback
pub fn apply_location_precedence(bundle: &mut AssetBundle, manifest: &ManifestDb) -> Result<()> {
    // Tier 1: Hardware EXIF GPS coordinates strictly override everything
    if let Some(coords) = bundle.gps_coords {
        if coords.source == LocationSource::ExifGps {
            return Ok(());
        }
    }

    // Tier 2: Explicit image-level override
    if let Some(img_loc) = manifest.get_bundle_location(&bundle.bundle_id)? {
        bundle.resolved_location_name = Some(img_loc.name);
        bundle.gps_coords = Some(GpsCoordinate {
            latitude: img_loc.latitude,
            longitude: img_loc.longitude,
            source: LocationSource::ImageLocationOverride,
        });
        return Ok(());
    }

    // Tier 3: Hierarchical directory/subdirectory override
    let dir_to_check = if let Some(ref rel) = bundle.relative_path {
        rel.parent().map(|p| p.to_path_buf())
    } else {
        bundle.primary_image.parent().map(|p| p.to_path_buf())
    };

    if let Some(ref dir_path) = dir_to_check {
        let is_subfolder = dir_path.components().count() > 1;
        if let Some(dir_loc) = manifest.get_hierarchical_directory_location(dir_path)? {
            let source = if is_subfolder {
                LocationSource::SubdirectoryGazetteer
            } else {
                LocationSource::TripFolderGazetteer
            };
            bundle.resolved_location_name = Some(dir_loc.name);
            bundle.gps_coords = Some(GpsCoordinate {
                latitude: dir_loc.latitude,
                longitude: dir_loc.longitude,
                source,
            });
            return Ok(());
        }
    }

    // Tier 4: Trip folder name fallback if present in directory_locations
    if let Some(trip_loc) = manifest.get_directory_location(&bundle.trip_folder_name)? {
        bundle.resolved_location_name = Some(trip_loc.name);
        bundle.gps_coords = Some(GpsCoordinate {
            latitude: trip_loc.latitude,
            longitude: trip_loc.longitude,
            source: LocationSource::TripFolderGazetteer,
        });
    }

    Ok(())
}

/// Authoritative resolved geographic location for a trip directory.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedLocation {
    pub name: String,
    pub region_code: String,
    pub latitude: f64,
    pub longitude: f64,
    pub source: String,
}

pub fn format_coord(lat: f64, lon: f64) -> String {
    let lat_dir = if lat >= 0.0 { 'N' } else { 'S' };
    let lon_dir = if lon >= 0.0 { 'E' } else { 'W' };
    format!("{:.3}°{}, {:.3}°{}", lat.abs(), lat_dir, lon.abs(), lon_dir)
}

pub fn resolve_from_override(
    override_str: &str,
    engine: &GazetteerEngine,
) -> Option<ResolvedLocation> {
    let parts: Vec<&str> = override_str.split(',').map(|s| s.trim()).collect();
    if parts.len() == 2 {
        if let (Ok(lat), Ok(lon)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
            return Some(ResolvedLocation {
                name: format!("{lat:.4}, {lon:.4}"),
                region_code: String::new(),
                latitude: lat,
                longitude: lon,
                source: "override_coord".to_string(),
            });
        }
    }

    let matches = engine.search(override_str, 1);
    if let Some((entry, _)) = matches.into_iter().next() {
        return Some(ResolvedLocation {
            name: entry.name,
            region_code: entry.region_code,
            latitude: entry.latitude,
            longitude: entry.longitude,
            source: "override_gazetteer".to_string(),
        });
    }

    None
}

/// Resolves geographic locations for trip directories using candidate tokens,
/// interactive disambiguation prompts, or non-interactive confident matching,
/// backed by persistent SQLite caching.
pub fn resolve_trip_locations<R: BufRead, W: Write>(
    trips_to_tokens: &[(String, Vec<String>)],
    engine: &GazetteerEngine,
    manifest: &ManifestDb,
    reader: &mut R,
    writer: &mut W,
    is_interactive: bool,
    location_override: Option<&str>,
) -> Result<HashMap<String, ResolvedLocation>> {
    let mut resolved_map = HashMap::new();

    // If an explicit location override was provided, apply it to all trips
    if let Some(override_str) = location_override {
        if let Some(loc) = resolve_from_override(override_str, engine) {
            for (trip_key, _) in trips_to_tokens {
                manifest.set_directory_location(trip_key, &loc)?;
                resolved_map.insert(trip_key.clone(), loc.clone());
            }
            return Ok(resolved_map);
        }
    }

    for (trip_key, candidate_tokens) in trips_to_tokens {
        // 1. Check SQLite cache first
        if let Some(cached) = manifest.get_directory_location(trip_key)? {
            resolved_map.insert(trip_key.clone(), cached);
            continue;
        }

        // 2. Query candidates from tokens
        let mut candidate_map: HashMap<
            (String, String),
            (crate::location::gazetteer::GazetteerEntry, f64),
        > = HashMap::new();

        for (token_idx, token) in candidate_tokens.iter().enumerate() {
            let matches = engine.search(token, 5);
            for (entry, score) in matches {
                // Leaf tokens (earlier in candidate list) receive a slight rank boost
                let position_weight = 1.0 - (token_idx as f64 * 0.02);
                let adjusted_score = score * position_weight;
                let key = (entry.name.clone(), entry.region_code.clone());
                candidate_map
                    .entry(key)
                    .and_modify(|existing| {
                        if adjusted_score > existing.1 {
                            *existing = (entry.clone(), adjusted_score);
                        }
                    })
                    .or_insert((entry, adjusted_score));
            }
        }

        let mut ranked: Vec<(crate::location::gazetteer::GazetteerEntry, f64)> =
            candidate_map.into_values().collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.name.cmp(&b.0.name))
        });
        ranked.truncate(4);

        if is_interactive {
            if !ranked.is_empty() {
                writeln!(writer, "\nResolve location for directory: \"{}\"", trip_key)?;
                writeln!(writer, "Candidate locations:")?;
                for (i, (entry, _score)) in ranked.iter().enumerate() {
                    let coord_str = format_coord(entry.latitude, entry.longitude);
                    let region_suffix = if entry.region_code.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", entry.region_code)
                    };
                    writeln!(
                        writer,
                        "  [{}] {}{} ({}) [{}]",
                        i + 1,
                        entry.name,
                        region_suffix,
                        coord_str,
                        entry.category
                    )?;
                }
                writeln!(writer, "  [s] Skip / No location")?;
                write!(
                    writer,
                    "Select option [1-{}], enter custom location/coords, or 's' to skip: ",
                    ranked.len()
                )?;
                writer.flush()?;

                let mut line = String::new();
                reader.read_line(&mut line)?;
                let trimmed = line.trim();

                if trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("s")
                    || trimmed.eq_ignore_ascii_case("skip")
                {
                    continue;
                }

                if let Ok(choice) = trimmed.parse::<usize>() {
                    if choice >= 1 && choice <= ranked.len() {
                        let entry = &ranked[choice - 1].0;
                        let loc = ResolvedLocation {
                            name: entry.name.clone(),
                            region_code: entry.region_code.clone(),
                            latitude: entry.latitude,
                            longitude: entry.longitude,
                            source: "interactive_gazetteer".to_string(),
                        };
                        manifest.set_directory_location(trip_key, &loc)?;
                        resolved_map.insert(trip_key.clone(), loc);
                        continue;
                    }
                }

                // If user entered custom text or coords:
                if let Some(custom_loc) = resolve_from_override(trimmed, engine) {
                    manifest.set_directory_location(trip_key, &custom_loc)?;
                    resolved_map.insert(trip_key.clone(), custom_loc);
                }
            } else {
                writeln!(
                    writer,
                    "\nNo automatic location matches for directory: \"{}\"",
                    trip_key
                )?;
                write!(
                    writer,
                    "Enter location name, coordinates (lat,lon), or press Enter to skip: "
                )?;
                writer.flush()?;

                let mut line = String::new();
                reader.read_line(&mut line)?;
                let trimmed = line.trim();

                if !trimmed.is_empty()
                    && !trimmed.eq_ignore_ascii_case("s")
                    && !trimmed.eq_ignore_ascii_case("skip")
                {
                    if let Some(custom_loc) = resolve_from_override(trimmed, engine) {
                        manifest.set_directory_location(trip_key, &custom_loc)?;
                        resolved_map.insert(trip_key.clone(), custom_loc);
                    }
                }
            }
        } else {
            // Non-interactive mode: pick top match if score > 0.85
            if let Some((top_entry, score)) = ranked.first() {
                if *score > 0.85 {
                    let loc = ResolvedLocation {
                        name: top_entry.name.clone(),
                        region_code: top_entry.region_code.clone(),
                        latitude: top_entry.latitude,
                        longitude: top_entry.longitude,
                        source: "auto_gazetteer".to_string(),
                    };
                    manifest.set_directory_location(trip_key, &loc)?;
                    resolved_map.insert(trip_key.clone(), loc);
                }
            }
        }
    }

    Ok(resolved_map)
}
