# Hexagonal Architecture Decoupling & Architecture Sensor Expansion Spec

**Date:** 2026-11-17  
**Author:** AI Agent & Engineer  
**Status:** PROPOSED  

---

## 1. Context & Motivation

Following the introduction of maintainability sensors (Martin Fowler's *Sensors for Coding Agents*), the codebase enforce automated boundaries on cognitive complexity, branch/statement limits, and basic module layering.

However, `src/organizer`—the core domain orchestration engine—currently imports directly from `crate::cli` (`OrganizeArgs`, `OrganizeAllArgs`, `RenderCatalogArgs`, `ReclassifyMode`). This violates the Dependency Inversion and Hexagonal Architecture principles:
- **Core Domain depending on Driving Adapter:** The CLI parser (`clap`) dictates the types used throughout the domain engine.
- **Inability to embed or test the domain independently:** Testing or embedding the organizer requires constructing CLI argument parser structs rather than pure domain configuration objects.
- **Leaky boundaries:** Lower modules could inadvertently import CLI concepts.

Furthermore, our architectural layering contract tests in `tests/test_suite/test_architecture_contracts.rs` only cover `types`, `storage`, and `catalog` decoupling. They do not enforce that:
1. `src/organizer` never imports `crate::cli`.
2. `src/db.rs` never imports `crate::organizer`, `crate::catalog`, or `crate::cli`.
3. `src/location` and `src/geo` never import `crate::organizer`, `crate::catalog`, or `crate::cli`.
4. `src/classifier` and `src/burst` never import `crate::organizer`, `crate::catalog`, or `crate::cli`.

---

## 2. Architectural Design: Strict Hexagonal Layering

We define a 4-tier unidirectional dependency hierarchy:

```
[ Tier 3: Driving Adapters (CLI) ]
               │
               ▼
[ Tier 2: Domain Orchestration (Organizer, Catalog) ]
               │
               ▼
[ Tier 1: Domain Subsystems (Storage, Scanner, Geo, Location, Classifier, Burst, DB) ]
               │
               ▼
[ Tier 0: Pure Leaf Domain Types (types.rs) ]
```

### Invariants:
1. **Tier 0 (`types.rs`)**: Pure domain leaf. Zero dependencies on higher tiers.
2. **Tier 1 (`db`, `geo`, `location`, `storage`, `classifier`, `burst`, `scanner`)**: May only depend on Tier 0 (`types.rs`) and other Tier 1 modules where functionally decoupled (e.g. `scanner` uses `db::ManifestDb`). NEVER depends on Tier 2 (`organizer`, `catalog`) or Tier 3 (`cli`).
3. **Tier 2 (`organizer`, `catalog`)**: Orchestrates Tier 1 subsystems. NEVER depends on Tier 3 (`cli`).
4. **Tier 3 (`cli`)**: Driving adapter. Parses CLI args and maps them into Tier 2 domain configuration structs or passes them to `organizer`.

---

## 3. Implementation Details

### A. Move Engine Configuration and Mode Enums to Core Domain
Move or re-export configuration structs into a new dedicated module or `types.rs`:
- Move `ReclassifyMode` from `src/cli.rs` to `src/types.rs`.
- Define clean domain options/config structs in `src/types.rs` or `src/organizer/mod.rs`:
  - `OrganizeOptions` / `OrganizeArgs`
  - `OrganizeAllOptions` / `OrganizeAllArgs`
  - `RenderCatalogOptions` / `RenderCatalogArgs`
- In `src/cli.rs`, use clap attributes to wrap or derive these structs, or re-export them from domain so CLI is an adapter, not the source of truth.

### B. Decouple `src/organizer` from `crate::cli`
- Replace all `use crate::cli::{...};` inside `src/organizer/` with `crate::types` or internal organizer config.
- `src/organizer` must have zero references to `crate::cli`.

### C. Expand Architectural Layering Contract Tests
In `tests/test_suite/test_architecture_contracts.rs`, add tests that verify:
1. `test_organizer_does_not_depend_on_cli`: asserts `src/organizer` has zero imports of `crate::cli`.
2. `test_db_does_not_depend_on_higher_layers`: asserts `src/db.rs` never imports `organizer`, `catalog`, or `cli`.
3. `test_location_and_geo_do_not_depend_on_higher_layers`: asserts `src/location/` and `src/geo.rs` never import `organizer`, `catalog`, or `cli`.
4. `test_classifier_and_burst_do_not_depend_on_higher_layers`: asserts `src/classifier.rs` and `src/burst.rs` never import `organizer`, `catalog`, or `cli`.

---

## 4. Verification Plan

1. **Pre-refactoring Red Tests (TDD)**:
   Add the architectural contract tests FIRST. `test_organizer_does_not_depend_on_cli` MUST fail initially because `src/organizer` currently imports `crate::cli`.
2. **Deterministic Behavior Verification**:
   All 454+ unit, integration, mock directory E2E, and Playwright UI tests must pass before and after the refactoring.
3. **Unified 8-Stage Pipeline**:
   Run `./scripts/verify.sh` to confirm zero clippy warnings, clean formatting, complete test pass, cargo deny check, Playwright smoke pass, and 0-warning rustdoc.
