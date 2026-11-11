---
title: "Batch Subdirectory Organization and Hierarchical Catalog Aggregation"
date: "2026-11-11"
category: "architecture-patterns"
module: "organizer"
problem_type: "architecture_pattern"
component: "batch-organizer"
severity: "medium"
applies_when:
  - "Organizing large root photo archives containing multiple distinct trip subdirectories"
  - "Isolating SQLite state, catalogs, and previews per subdirectory while providing root aggregation"
  - "Ensuring fault tolerance and error containment during multi-directory batch operations"
  - "Resolving and displaying unambiguous full directory paths in interactive location prompts"
tags:
  - "organize-all"
  - "batch-processing"
  - "fault-tolerance"
  - "manifest-rollup"
  - "hierarchical-catalogs"
---

# Batch Subdirectory Organization and Hierarchical Catalog Aggregation

## Context
When running photo organization on an expansive archive root (e.g. `/Volumes/pictures/` spanning dozens of annual or destination trips), executing a monolithic single-pass `organize` presents operational challenges:
1. A single corrupted raw file or interrupt halts processing for the entire multi-gigabyte library.
2. Browsing requires navigating a massive monolithic catalog rather than inspecting individual trip catalogs (`<trip>/catalog.html`).
3. Disambiguating trip locations during interactive prompts becomes error-prone when multiple trips contain identically named leaf directories (e.g. `scenery/`, `safari 1/`, `processed/`).

## Guidance

### 1. Per-Subdirectory Isolation with Fault Tolerance (`organize-all`)
`avicull organize-all` implements sequential isolation:
- Scans immediate non-hidden, non-system child directories of `--input`, skipping reserved paths (`birds_hierarchy`, `.avicull`, `@eaDir`, `.photoslibrary`).
- Runs full organization sequentially on each child directory with isolated arguments (`--input <child_dir>`), giving each subdirectory its own self-contained `.avicull/manifest.sqlite` and `<child_dir>/catalog.html`.
- Implements error containment: failures in one child subdirectory are logged as warnings and aggregated, allowing subsequent subdirectories to continue without terminating the entire batch job.

### 2. Upward Manifest Rollup and Aggregated Root Catalog
Following sequential child runs, `organize-all` synthesizes the root view:
- Opens or creates the root manifest at `<input>/.avicull/manifest.sqlite`.
- Uses `discover_child_manifests` and `import_child_manifest` to pull bundles, decisions, and location overrides into the root database with zero file movement.
- Calls `regenerate_catalog_from_manifest` to produce `<input>/catalog.html`, allowing the photographer to browse the entire archive holistically while preserving individual trip catalogs.

### 3. Unambiguous Full Path Display in Location Disambiguation Prompts
When prompting the user for location overrides or gazetteer choices:
- Photographers frequently have nested subfolders sharing common generic names (`safari 1`, `selects`, `day 2`).
- Prompts must format the full resolved path (`Resolve location for directory: "2025/Ladakh/scenery"`) rather than truncating to leaf components, providing clear contextual clarity during interactive review.
