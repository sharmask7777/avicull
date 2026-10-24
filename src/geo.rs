use crate::types::{AssetBundle, GpsCoordinate, LocationSource};
use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TomlLocation {
    lat: f64,
    lon: f64,
}

/// Offline Important Bird Area (IBA) and prominent birding hotspot coordinates.
static BUILTIN_GAZETTEER: &[(&str, f64, f64)] = &[
    ("bharatpur", 27.1750, 77.5250),
    ("keoladeo", 27.1750, 77.5250),
    ("ranganathittu", 12.4244, 76.6563),
    ("leh", 34.1526, 77.5771),
    ("ladakh", 34.1526, 77.5771),
    ("chilika", 19.6800, 85.3200),
    ("kaziranga", 26.5775, 93.1711),
    ("corbett", 29.5300, 78.7747),
    ("tadoba", 20.2444, 79.3014),
    ("kanha", 22.3345, 80.6115),
    ("gir", 21.1243, 70.8242),
    ("sundarbans", 21.9497, 89.1833),
    ("kutch", 23.7337, 69.8597),
    ("manglajodi", 19.9238, 85.4297),
    ("nalsarovar", 22.8000, 72.0300),
    ("pointcalimere", 10.3000, 79.8667),
    ("vedanthangal", 12.5458, 79.8550),
    ("thattekad", 10.1333, 76.6833),
    ("pulicat", 13.6667, 80.1667),
    ("sultanpur", 28.4617, 76.8928),
    ("okhla", 28.5447, 77.3090),
    ("panna", 24.7200, 80.2000),
    ("bandhavgarh", 23.7000, 81.0300),
    ("ranthambore", 26.0173, 76.5026),
    ("everglades", 25.2866, -80.8987),
    ("galapagos", -0.9538, -90.9656),
    ("pantanal", -16.4800, -56.6300),
    ("kruger", -23.9884, 31.5547),
    ("serengeti", -2.3333, 34.8333),
];

/// Extracts EXIF GPS and timestamp from an image file if available.
pub fn extract_exif_metadata(path: &Path) -> (Option<GpsCoordinate>, Option<i64>) {
    let mut coords: Option<GpsCoordinate> = None;
    let mut timestamp_ms: Option<i64> = None;

    // Method 1: Try nom-exif parse_metadata
    if let Ok(file) = File::open(path) {
        let mut buf_reader = BufReader::new(file);
        #[allow(deprecated)]
        if let Ok(entries) = nom_exif::parse_metadata(&mut buf_reader) {
            let mut lat: Option<f64> = None;
            let mut lon: Option<f64> = None;

            for (key, val) in entries {
                let lower_key = key.to_lowercase();
                if lower_key.contains("gpslatitude") && !lower_key.contains("ref") {
                    if let nom_exif::EntryValue::F64(v) = val {
                        lat = Some(v);
                    }
                } else if lower_key.contains("gpslongitude") && !lower_key.contains("ref") {
                    if let nom_exif::EntryValue::F64(v) = val {
                        lon = Some(v);
                    }
                } else if lower_key.contains("datetimeoriginal") || lower_key.contains("create") {
                    match val {
                        nom_exif::EntryValue::Time(t) => {
                            timestamp_ms = Some(t.timestamp_millis());
                        }
                        nom_exif::EntryValue::NaiveDateTime(ndt) => {
                            timestamp_ms = Some(ndt.and_utc().timestamp_millis());
                        }
                        nom_exif::EntryValue::Text(s) => {
                            if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, "%Y:%m:%d %H:%M:%S")
                            {
                                timestamp_ms = Some(ndt.and_utc().timestamp_millis());
                            }
                        }
                        _ => {}
                    }
                }
            }

            if let (Some(la), Some(lo)) = (lat, lon) {
                coords = Some(GpsCoordinate {
                    latitude: la,
                    longitude: lo,
                    source: LocationSource::ExifGps,
                });
            }
        }
    }

    // Method 2: Try kamadak-exif (exif crate)
    if coords.is_none() || timestamp_ms.is_none() {
        if let Ok(file) = File::open(path) {
            let mut buf_reader = BufReader::new(file);
            if let Ok(exif_reader) = exif::Reader::new().read_from_container(&mut buf_reader) {
                if coords.is_none() {
                    let lat_field =
                        exif_reader.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY);
                    let lat_ref =
                        exif_reader.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY);
                    let lon_field =
                        exif_reader.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY);
                    let lon_ref =
                        exif_reader.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY);

                    if let (Some(la_f), Some(la_r), Some(lo_f), Some(lo_r)) =
                        (lat_field, lat_ref, lon_field, lon_ref)
                    {
                        if let (Some(la), Some(lo)) = (
                            parse_kamadak_dms(&la_f.value, &la_r.display_value().to_string()),
                            parse_kamadak_dms(&lo_f.value, &lo_r.display_value().to_string()),
                        ) {
                            coords = Some(GpsCoordinate {
                                latitude: la,
                                longitude: lo,
                                source: LocationSource::ExifGps,
                            });
                        }
                    }
                }

                if timestamp_ms.is_none() {
                    if let Some(dto) =
                        exif_reader.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
                    {
                        let s = dto.display_value().to_string();
                        if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                            timestamp_ms = Some(ndt.and_utc().timestamp_millis());
                        } else if let Ok(ndt) =
                            NaiveDateTime::parse_from_str(&s, "%Y:%m:%d %H:%M:%S")
                        {
                            timestamp_ms = Some(ndt.and_utc().timestamp_millis());
                        }
                    }
                }
            }
        }
    }

    // Method 3: Fallback timestamp to file mtime if EXIF has no timestamp
    if timestamp_ms.is_none() {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    timestamp_ms = Some(dur.as_millis() as i64);
                }
            }
        }
    }

    (coords, timestamp_ms)
}

fn parse_kamadak_dms(val: &exif::Value, ref_str: &str) -> Option<f64> {
    if let exif::Value::Rational(rationals) = val {
        if rationals.len() >= 3 {
            let deg = rationals[0].num as f64 / rationals[0].denom as f64;
            let min = rationals[1].num as f64 / rationals[1].denom as f64;
            let sec = rationals[2].num as f64 / rationals[2].denom as f64;
            let mut dec = deg + min / 60.0 + sec / 3600.0;
            if ref_str.starts_with('S') || ref_str.starts_with('W') {
                dec = -dec;
            }
            return Some(dec);
        }
    }
    None
}

/// Tokenizes a folder name for gazetteer lookup.
fn tokenize_folder_name(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// Loads a `locations.toml` file into a map of keyword -> (lat, lon).
pub fn load_locations_toml(path: &Path) -> Result<HashMap<String, (f64, f64)>> {
    let content = fs::read_to_string(path)?;
    let parsed: HashMap<String, TomlLocation> = toml::from_str(&content)?;
    let mut map = HashMap::new();
    for (k, v) in parsed {
        map.insert(k.to_lowercase(), (v.lat, v.lon));
    }
    Ok(map)
}

/// Resolves locations across all bundles adhering to the 4 priorities:
/// 1. EXIF GPS
/// 2. Sibling EXIF propagation (median GPS within same trip folder)
/// 3. Trip folder name gazetteer (TOML mapping or built-in IBA table)
/// 4. Fallback CLI default coordinates
pub fn resolve_locations(
    bundles: &mut [AssetBundle],
    locations_toml_path: Option<&Path>,
    default_coords: Option<(f64, f64)>,
) -> Result<()> {
    // Step 1: For bundles missing EXIF GPS or timestamps, attempt extraction from primary or companion images
    for bundle in bundles.iter_mut() {
        if bundle.gps_coords.is_none() || bundle.capture_timestamp_ms.is_none() {
            let (gps, ts) = extract_exif_metadata(&bundle.primary_image);
            if bundle.gps_coords.is_none() {
                bundle.gps_coords = gps;
            }
            if bundle.capture_timestamp_ms.is_none() {
                bundle.capture_timestamp_ms = ts;
            }

            if bundle.gps_coords.is_none() || bundle.capture_timestamp_ms.is_none() {
                for comp in &bundle.companion_images {
                    let (cgps, cts) = extract_exif_metadata(comp);
                    if bundle.gps_coords.is_none() && cgps.is_some() {
                        bundle.gps_coords = cgps;
                    }
                    if bundle.capture_timestamp_ms.is_none() && cts.is_some() {
                        bundle.capture_timestamp_ms = cts;
                    }
                    if bundle.gps_coords.is_some() && bundle.capture_timestamp_ms.is_some() {
                        break;
                    }
                }
            }
        }
    }

    // Step 2: Sibling propagation within each trip folder
    let mut trip_gps_medians: HashMap<String, (f64, f64)> = HashMap::new();
    let mut trip_lats: HashMap<String, Vec<f64>> = HashMap::new();
    let mut trip_lons: HashMap<String, Vec<f64>> = HashMap::new();

    for bundle in bundles.iter() {
        if let Some(gps) = bundle.gps_coords {
            if gps.source == LocationSource::ExifGps {
                trip_lats
                    .entry(bundle.trip_folder_name.clone())
                    .or_default()
                    .push(gps.latitude);
                trip_lons
                    .entry(bundle.trip_folder_name.clone())
                    .or_default()
                    .push(gps.longitude);
            }
        }
    }

    for (trip, mut lats) in trip_lats {
        if let Some(mut lons) = trip_lons.remove(&trip) {
            lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            lons.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = lats.len() / 2;
            let med_lat = lats[mid];
            let med_lon = lons[mid];
            trip_gps_medians.insert(trip, (med_lat, med_lon));
        }
    }

    for bundle in bundles.iter_mut() {
        if bundle.gps_coords.is_none() {
            if let Some(&(med_lat, med_lon)) = trip_gps_medians.get(&bundle.trip_folder_name) {
                bundle.gps_coords = Some(GpsCoordinate {
                    latitude: med_lat,
                    longitude: med_lon,
                    source: LocationSource::SiblingPropagation,
                });
            }
        }
    }

    // Step 3: Trip Folder Name Gazetteer (locations.toml, then built-in IBA table)
    let toml_map = if let Some(p) = locations_toml_path {
        load_locations_toml(p).ok()
    } else {
        None
    };

    for bundle in bundles.iter_mut() {
        if bundle.gps_coords.is_none() {
            let mut tokens = tokenize_folder_name(&bundle.trip_folder_name);
            for comp in bundle.primary_image.components() {
                tokens.extend(tokenize_folder_name(&comp.as_os_str().to_string_lossy()));
            }
            let mut resolved: Option<(f64, f64)> = None;

            // Check toml mapping
            if let Some(ref map) = toml_map {
                for token in &tokens {
                    if let Some(&coords) = map.get(token) {
                        resolved = Some(coords);
                        break;
                    }
                }
            }

            // Check built-in table
            if resolved.is_none() {
                for token in &tokens {
                    for &(hotspot, lat, lon) in BUILTIN_GAZETTEER {
                        if token == hotspot
                            || token.contains(hotspot)
                            || hotspot.contains(token.as_str())
                        {
                            resolved = Some((lat, lon));
                            break;
                        }
                    }
                    if resolved.is_some() {
                        break;
                    }
                }
            }

            if let Some((lat, lon)) = resolved {
                bundle.gps_coords = Some(GpsCoordinate {
                    latitude: lat,
                    longitude: lon,
                    source: LocationSource::TripFolderGazetteer,
                });
            }
        }
    }

    // Step 4: Fallback CLI Default Coordinates
    if let Some((def_lat, def_lon)) = default_coords {
        for bundle in bundles.iter_mut() {
            if bundle.gps_coords.is_none() {
                bundle.gps_coords = Some(GpsCoordinate {
                    latitude: def_lat,
                    longitude: def_lon,
                    source: LocationSource::UserDefaultRegion,
                });
            }
        }
    }

    Ok(())
}
