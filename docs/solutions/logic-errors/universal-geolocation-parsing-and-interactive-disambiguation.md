---
module: location
tags: [geolocation, parser, dms, google-maps, terminal-ux, gazetteer]
problem_type: logic-errors
---

# Universal Geolocation Parsing and Interactive Disambiguation UX

## Problem

Photographers and users obtaining coordinates from web searches or mapping tools often encounter brittle location parsing when setting trip or directory locations. In `avicull`, setting a directory location via `--location` or the interactive terminal prompt required strict, plain `f64, f64` decimal pairs (e.g. `34.1526, 77.5771`). 

When users pasted real-world formats—such as Google Search summary snippets (`34.1526° N, 77.5771° E`), camera EXIF Degrees Minutes Seconds (`34° 09' 09.36" N, 77° 34' 37.56" E`), Google Maps URLs (`https://maps.google.com/?q=...` or `/@lat,lon,zoom`), or inadvertently typed longitude first—the input failed numerical parsing and silently fell back to fuzzy trigram string matching against the offline gazetteer, which invariably failed or matched an irrelevant distant place name.

Furthermore, the interactive terminal prompt lacked transparent feedback: it printed cryptic guidance, gave no examples of acceptable input styles, and did not echo back what the engine interpreted or which regional eBird checklist prior was selected.

## Root Cause

1. **Brittle String Splitting & Parsing**: [`resolve_from_override`](src/location/resolver.rs) performed a naive `split(',')` expecting exactly two tokens that cleanly parsed with `.parse::<f64>()`. Any degree symbols (`°`), minutes/seconds symbols (`'`, `"`), cardinal letters (`N`, `S`, `E`, `W`), whitespace variations, or URL structures caused parsing to return `None`.
2. **Missing Inverted Axis Normalization**: Users often copy `(lon, lat)` pairs or write coordinates in longitude-first order. Because latitudes must be in $[-90.0, 90.0]$ and longitudes in $[-180.0, 180.0]$, values where $|\text{val}_1| > 90$ caused silent rejection even when the user intent was unambiguous.
3. **Opaque Prompt Guidance**: The interactive prompt in [`resolve_trip_locations`](src/location/resolver.rs) presented a terse instruction without concrete format examples and without confirming what coordinate and administrative region were actually resolved before persisting them to the SQLite manifest.

## Solution

### 1. Dedicated Coordinate & URL Parser (`src/location/coord_parser.rs`)

A dedicated `parse_coordinates` parser was introduced to normalize and extract `(latitude, longitude)` across heterogeneous inputs:
- **Google Maps URLs**: Scans query params (`q=lat,lon`) and path coordinates (`/@lat,lon,...`) using non-greedy extraction.
- **Cardinal & DMS Parsing**: Normalizes degree symbols (`°`), minutes (`'`), and seconds (`"`), computing decimal degrees as $\text{deg} + \frac{\text{min}}{60} + \frac{\text{sec}}{3600}$, and applies negative signs when `S` or `W` cardinal indicators are present.
- **Automatic Axis Swapping**: If $\text{val}_1$ exceeds $90^\circ$ but is $\le 180^\circ$ and $\text{val}_2 \in [-90^\circ, 90^\circ]$, the parser safely swaps the values into valid `(latitude, longitude)`.
- **Clean Slicing & Cardinal Separation**: Handles whitespace-separated and delimiter-free cardinal tokens (e.g. `34.15N 77.58E`).

### 2. Immediate Confirmation & Rich Prompting (`src/location/resolver.rs`)

The interactive resolver now provides:
- Concrete examples of coordinates, DMS strings, Google Maps URLs, and place names directly above the input prompt.
- Immediate terminal echo of resolved coordinates, nearest gazetteer hotspot, distance in km, and regional eBird checklist code:
  ```text
  ✔ Resolved: "Leh" (34.153°N, 77.577°E) [Region: IN-LA]
  ```
- Clear diagnostic warnings (`⚠ Could not resolve location from "..."`) when an input cannot be mapped.

## Prevention & Invariants

- **Accept Any Geographic Representation**: CLI flags and interactive prompts that accept geographic coordinates must never assume clean decimal-only inputs; always support standard Google Maps links, DMS notation, and cardinal degrees.
- **Validate Invariants via Dedicated Unit Tests**: Verify edge cases (Pacific ocean cutoff, Bangkok inverted coordinates, DMS cardinal negation) in the consolidated test suite (`tests/test_suite/test_location_resolver.rs`).
