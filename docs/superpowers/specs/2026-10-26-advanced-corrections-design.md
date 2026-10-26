# Advanced Classification Corrections & Reconciliation Design

## 1. Intent & Context
Following the implementation of durable SQLite manifests and user overrides (Issue #8), we identified four key areas to make classification corrections and reconciliation significantly more robust, intuitive, and error-tolerant:

1. **Ambiguity Detection & Disambiguation**: When multiple photo bundles match a query (e.g. `--file DSC_0042` across multiple trips in a parent archive), prevent silent arbitrary choice; prompt interactively or error out with clear disambiguation instructions.
2. **In-Place Rename Move Reconciliation**: Detect manual filename edits in `--mode in-place` (e.g. user renamed `DSC_0042_Mallard.CR2` to `DSC_0042_Northern_Pintail.CR2`), promote them to durable overrides, and rename companions/sidecars lockstep.
3. **Burst Propagation (`--propagate-burst` / `-b`)**: Allow a correction on one photo to automatically reclassify and move/rename all other photos belonging to the same burst.
4. **Fuzzy Species Autocorrect / Typo Suggestions**: When a user inputs an unrecognized species name, query the eBird taxonomy using trigram similarity and suggest or prompt for the closest match.

## 2. Technical Design

### A. Ambiguity Detection & Disambiguation (`src/organizer/correct.rs`)
- Filter all candidate bundles matching the query.
- If matches == 0: return helpful error.
- If matches == 1: proceed with that bundle.
- If matches > 1:
  - If interactive terminal (`is_terminal()` and `!no_prompt`): display matched bundles (trip folder, original stem, capture date, current species) and prompt the user to select by index.
  - If non-interactive: return an error listing the candidates and asking the user to specify by relative path (e.g. `--file "Leh/DSC_0042.CR2"`).

### B. In-Place Move Reconciliation (`src/organizer/reconcile.rs`)
- When running in in-place mode (`args.mode == OrganizationMode::InPlaceFileRenameToDescriptiveNames`), scan the directory.
- For each image file, decompose its filename using `decompose_filename`.
- If the stem corresponds to an existing bundle stem, but the attached species tags indicate a different species from `existing_decision.assigned_species_slug`:
  - Register as a user override (`is_user_override = 1`, `final_confidence = 1.0`).
  - Rename any companion images or sidecars that still have the old name.
  - Update SQLite `bundles` and `hardlink_operations` tables.

### C. Burst Propagation (`--propagate-burst` / `--burst`)
- Add CLI flag `--propagate-burst` (alias `--burst`, `-b`) to `CorrectArgs`.
- If set, find all bundles with matching `burst_id == target_bundle.burst_id` (if `burst_id.is_some()`).
- In interactive terminal if `--propagate-burst` is not passed but photo has a burst of >1 photos, prompt: `"Photo is part of a N-photo burst (burst_id). Reclassify entire burst? [y/N]"`.
- Apply correction across all matched burst bundles, moving/renaming each and recording operations.

### D. Fuzzy Species Autocorrect / Suggestions
- Query embedded eBird taxonomy / gazetteer trigram engine for species names.
- If input species is not an exact common name, folder slug, or scientific name:
  - Find top 1 closest candidate above similarity threshold (e.g. 0.6).
  - In interactive mode: prompt `"Species '{input}' not found. Did you mean '{candidate}'? [Y/n]"`.
  - In non-interactive mode: accept input as custom slug, but log/warn if a close official match exists.
