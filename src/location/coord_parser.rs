//! # Geolocation & Coordinate String Parser
//!
//! Provides parsing for real-world user coordinate inputs across formats:
//! - Google Maps URLs (e.g. `https://maps.google.com/?q=34.1526,77.5771` or `@34.1526,77.5771,15z`)
//! - Degrees with cardinal directions (e.g. `34.1526° N, 77.5771° E` or `34.1526N 77.5771E`)
//! - DMS (Degrees, Minutes, Seconds) (e.g. `34° 09' 09.36" N, 77° 34' 37.56" E`)
//! - Signed decimals with automatic lat/lon inversion detection (e.g. `34.1526, 77.5771` or `77.5771, 34.1526`)

/// Extracted coordinate pair representing `(latitude, longitude)` in decimal degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedCoordinates {
    /// Latitude in range `[-90.0, 90.0]`
    pub latitude: f64,
    /// Longitude in range `[-180.0, 180.0]`
    pub longitude: f64,
}

/// Attempts to parse geographic coordinates from an arbitrary user input string,
/// supporting Google Maps URLs, cardinal degrees, DMS, and signed decimals.
///
/// # Arguments
///
/// * `input` - Raw text provided by user via CLI, interactive prompt, or URL.
///
/// # Returns
///
/// `Some(ParsedCoordinates)` if valid coordinates could be extracted, `None` otherwise.
pub fn parse_coordinates(input: &str) -> Option<ParsedCoordinates> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Google Maps URLs
    if let Some(coords) = parse_google_maps_url(trimmed) {
        return Some(coords);
    }

    // 2. DMS or cardinal directions (contains °, ', ", N, S, E, or W)
    if trimmed.contains('°')
        || trimmed.contains('\'')
        || trimmed.contains('"')
        || contains_cardinal_indicator(trimmed)
    {
        if let Some(coords) = parse_cardinal_or_dms(trimmed) {
            return Some(coords);
        }
    }

    // 3. Simple decimal pair (e.g. "34.1526, 77.5771" or "34.1526 77.5771")
    parse_decimal_pair(trimmed)
}

/// Extracts coordinates from Google Maps URLs:
/// - `.../?q=34.1526,77.5771` or `.../search/34.1526,77.5771`
/// - `.../@34.1526,77.5771,15z...`
fn parse_google_maps_url(url: &str) -> Option<ParsedCoordinates> {
    if !url.contains("maps.google.")
        && !url.contains("google.com/maps")
        && !url.contains("maps.app.goo.gl")
    {
        return None;
    }

    // Pattern 1: `?q=lat,lon` or `&q=lat,lon` or `/place/lat,lon`
    if let Some(q_idx) = url.find("q=") {
        let after_q = &url[q_idx + 2..];
        let end_idx = after_q.find('&').unwrap_or(after_q.len());
        let candidate = &after_q[..end_idx];
        if let Some(coords) = parse_decimal_pair(candidate) {
            return Some(coords);
        }
    }

    // Pattern 2: `/@lat,lon,zoom`
    if let Some(at_idx) = url.find("/@") {
        let after_at = &url[at_idx + 2..];
        let parts: Vec<&str> = after_at.split(',').collect();
        if parts.len() >= 2 {
            if let (Ok(lat), Ok(lon)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                return validate_and_normalize(lat, lon);
            }
        }
    }

    // Pattern 3: `/place/Name/@lat,lon`
    if let Some(at_idx) = url.find('@') {
        let after_at = &url[at_idx + 1..];
        let parts: Vec<&str> = after_at.split(',').collect();
        if parts.len() >= 2 {
            if let (Ok(lat), Ok(lon)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                return validate_and_normalize(lat, lon);
            }
        }
    }

    None
}

/// Checks if string has cardinal direction letters (N/S/E/W) as standalone or attached tokens.
fn contains_cardinal_indicator(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    upper
        .chars()
        .any(|c| c == 'N' || c == 'S' || c == 'E' || c == 'W')
}

/// Parses DMS or Cardinal decimal strings like `34.1526° N, 77.5771° E` or `34° 09' 09.36" N, 77° 34' 37.56" E`.
fn parse_cardinal_or_dms(text: &str) -> Option<ParsedCoordinates> {
    // If separated by comma or semicolon, split into lat and lon parts
    let parts: Vec<&str> = if text.contains(',') {
        text.split(',').collect()
    } else if text.contains(';') {
        text.split(';').collect()
    } else {
        // Fallback: look for boundary between cardinal letters e.g. "34.1N 77.1E"
        // Try to split around the first occurrence of N/S followed by space or number
        let upper = text.to_ascii_uppercase();
        let idx = upper.find('N').or_else(|| upper.find('S'))?;
        vec![&text[..=idx], &text[idx + 1..]]
    };

    if parts.len() != 2 {
        return None;
    }

    let val1 = parse_single_component(parts[0])?;
    let val2 = parse_single_component(parts[1])?;

    // Determine which is lat and which is lon
    let part0_upper = parts[0].to_ascii_uppercase();
    let part1_upper = parts[1].to_ascii_uppercase();

    let is_part0_lat = part0_upper.contains('N') || part0_upper.contains('S');
    let is_part0_lon = part0_upper.contains('E') || part0_upper.contains('W');
    let is_part1_lon = part1_upper.contains('E') || part1_upper.contains('W');

    if (!is_part0_lon || is_part0_lat) && is_part1_lon {
        validate_and_normalize(val1, val2)
    } else if is_part0_lon {
        validate_and_normalize(val2, val1)
    } else {
        // Neither explicitly specified or both ambiguous: assume (lat, lon)
        validate_and_normalize(val1, val2)
    }
}

/// Parses a single DMS or cardinal coordinate component (e.g. `34° 09' 09.36" N` or `77.5771° E`).
fn parse_single_component(comp: &str) -> Option<f64> {
    let trimmed = comp.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_negative = upper.contains('S') || upper.contains('W') || trimmed.starts_with('-');

    // Clean out degrees, minutes, seconds symbols and cardinal letters
    let cleaned: String = trimmed
        .chars()
        .map(|c| match c {
            '°' | '\'' | '"' | 'N' | 'n' | 'S' | 's' | 'E' | 'e' | 'W' | 'w' => ' ',
            _ => c,
        })
        .collect();

    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    let abs_val = match tokens.len() {
        1 => tokens[0].parse::<f64>().ok()?,
        2 => {
            let deg = tokens[0].parse::<f64>().ok()?;
            let min = tokens[1].parse::<f64>().ok()?;
            deg.abs() + (min / 60.0)
        }
        3 => {
            let deg = tokens[0].parse::<f64>().ok()?;
            let min = tokens[1].parse::<f64>().ok()?;
            let sec = tokens[2].parse::<f64>().ok()?;
            deg.abs() + (min / 60.0) + (sec / 3600.0)
        }
        _ => return None,
    };

    Some(if is_negative { -abs_val } else { abs_val })
}

/// Parses a simple decimal pair like "34.1526, 77.5771" or "34.1526 77.5771"
fn parse_decimal_pair(input: &str) -> Option<ParsedCoordinates> {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            ',' | ';' | '\t' => ' ',
            _ => c,
        })
        .collect();

    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.len() != 2 {
        return None;
    }

    let val1 = tokens[0].parse::<f64>().ok()?;
    let val2 = tokens[1].parse::<f64>().ok()?;

    validate_and_normalize(val1, val2)
}

/// Validates coordinate ranges and auto-swaps if `(lon, lat)` was passed instead of `(lat, lon)`.
fn validate_and_normalize(val1: f64, val2: f64) -> Option<ParsedCoordinates> {
    // Standard case: val1 is latitude in [-90, 90], val2 is longitude in [-180, 180]
    if (-90.0..=90.0).contains(&val1) && (-180.0..=180.0).contains(&val2) {
        return Some(ParsedCoordinates {
            latitude: val1,
            longitude: val2,
        });
    }

    // Inverted case: val1 is outside [-90, 90] but within [-180, 180], and val2 is within [-90, 90]
    if (val1.abs() > 90.0 && val1.abs() <= 180.0) && (-90.0..=90.0).contains(&val2) {
        return Some(ParsedCoordinates {
            latitude: val2,
            longitude: val1,
        });
    }

    None
}
