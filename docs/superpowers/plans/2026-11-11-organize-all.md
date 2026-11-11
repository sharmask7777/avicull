# `organize-all` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `avicull organize-all` CLI command to sequentially organize immediate subdirectories into independent SQLite manifests and `catalog.html` files, followed by an automatic root rollup catalog.

**Architecture:** Refactor `Commands::Organize` in `src/main.rs` into a reusable library function `run_organize(args: &OrganizeArgs) -> Result<()>`. Implement `run_organize_all(args: &OrganizeAllArgs) -> Result<()>` in `src/organizer/batch.rs` to discover immediate subdirectories, apply strict directory skip rules, sequentially dispatch `run_organize`, and execute the parent roll-up.

**Tech Stack:** Rust 2021, Clap v4, Rusqlite, Rayon, Tempfile

**Spec:** [`docs/superpowers/specs/2026-11-11-organize-all-design.md`](file:///Users/shaleensharma/workplace/gemini_ws/avian_rearranger/docs/superpowers/specs/2026-11-11-organize-all-design.md)

## Global Constraints
- Strictly adhere to `AGENTS.md` and `CODING_STANDARDS.md`.
- No `unsafe`, no `.unwrap()`, no `.expect()`, no `panic!()` in production code.
- Source immutability preserved under default `--mode in-place`.
- Exclude `birds_hierarchy`, `@eaDir`, `*.photoslibrary`, `.avicull`, and hidden directories (`.*`).
- Zero warnings on `cargo clippy --all-targets --all-features -- -D warnings`.
- Zero warnings on `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.

## Review Focus
1. **Empty subdirectories or directories without images**: Must initialize `.avicull/manifest.sqlite` and `catalog.html` without error.
2. **Special and hidden directories**: Must ignore `.git`, `.avicull`, `birds_hierarchy`, `@eaDir`, and `*.photoslibrary`.
3. **Spaces and special characters in folder names**: Paths with spaces (e.g. `Ladakh 2025`) must resolve cleanly.
4. **Idempotent fast-path**: Re-running on unchanged subdirectories completes in milliseconds via existing delta checks without re-inferring photos.
5. **Parent catalog rollup integrity**: Root catalog references assets in child subdirectories with correct relative paths.

---

### Task 1: CLI Arguments Definition (`src/cli.rs`)

**Files:**
- Modify: `src/cli.rs:30-70`, `src/cli.rs:270-350`
- Test: `tests/test_suite/test_organize_all.rs`

**Interfaces:**
- Produces: `Commands::OrganizeAll(OrganizeAllArgs)` and struct `OrganizeAllArgs`

- [ ] **Step 1: Write the failing test**
Create `tests/test_suite/test_organize_all.rs` testing CLI parsing of `organize-all` with default and custom arguments. Register `mod test_organize_all;` in `tests/test_suite/main.rs`.

```rust
use clap::Parser;
use avicull::cli::{Cli, Commands};
use std::path::PathBuf;

#[test]
fn test_cli_parse_organize_all_defaults() {
    let args = Cli::try_parse_from(["avicull", "organize-all"]).unwrap();
    match args.command {
        Commands::OrganizeAll(all_args) => {
            assert_eq!(all_args.input, PathBuf::from("."));
            assert!(!all_args.no_parent_catalog);
            assert!(!all_args.verbose);
            assert!(!all_args.force);
        }
        _ => panic!("Expected Commands::OrganizeAll"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_suite test_cli_parse_organize_all_defaults`
Expected: FAIL with variant `OrganizeAll` or `OrganizeAllArgs` not found.

- [ ] **Step 3: Implement `OrganizeAllArgs` in `src/cli.rs` and add variant to `Commands`**
Add `OrganizeAllArgs` with fields: `input`, `output`, `no_parent_catalog`, `mode`, `no_prompt`, `force`, `verbose`, `threads`, `models_dir`, `default_lat`, `default_lon`, `locations_file`, `min_confidence`, `min_detect`. Add `Commands::OrganizeAll(OrganizeAllArgs)`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_suite test_cli_parse_organize_all_defaults`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add src/cli.rs tests/test_suite/test_organize_all.rs tests/test_suite/main.rs
git commit -m "feat(cli): define organize-all subcommand and arguments"
```

---

### Task 2: Refactor `run_organize` into Reusable Engine (`src/organizer/mod.rs` & `src/main.rs`)

**Files:**
- Create: `src/organizer/engine.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/main.rs`
- Test: `tests/test_suite/test_spec_conformance.rs`

**Interfaces:**
- Produces: `pub fn run_organize(args: &OrganizeArgs) -> anyhow::Result<()>` in `src/organizer/mod.rs`

- [ ] **Step 1: Write the failing test**
Add a test in `tests/test_suite/test_organize_all.rs` calling `avicull::organizer::run_organize` directly on a temporary directory with empty photos and asserting `.avicull/manifest.sqlite` and `catalog.html` exist.

```rust
#[test]
fn test_run_organize_function_interface() {
    let temp = tempfile::tempdir().unwrap();
    let args = avicull::cli::OrganizeArgs {
        input: temp.path().to_path_buf(),
        output: None,
        dry_run: false,
        force: false,
        reclassify: false,
        min_confidence: 0.65,
        min_detect: 0.25,
        burst_window_ms: 3000,
        confidence_mode: "burst-consensus".to_string(),
        mode: avicull::types::OrganizationMode::InPlace,
        geo_prior_weight: 1.0,
        locations_file: None,
        default_lat: None,
        default_lon: None,
        models_dir: None,
        threads: Some(1),
        location: None,
        no_prompt: true,
        max_cache_size: "5GB".to_string(),
        verbose: false,
    };
    avicull::organizer::run_organize(&args).unwrap();
    assert!(temp.path().join(".avicull/manifest.sqlite").exists());
    assert!(temp.path().join("catalog.html").exists());
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_suite test_run_organize_function_interface`
Expected: FAIL with `run_organize` not found.

- [ ] **Step 3: Extract `run_organize` from `src/main.rs` to `src/organizer/engine.rs`**
Expose `pub fn run_organize(args: &OrganizeArgs) -> Result<()>` in `src/organizer/mod.rs`. Update `src/main.rs` to call `avicull::organizer::run_organize(&args)` in `Commands::Organize`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_suite test_run_organize_function_interface`
Expected: PASS

- [ ] **Step 5: Run existing test suite to ensure no regressions**
Run: `cargo test --test test_suite`
Expected: PASS

- [ ] **Step 6: Commit**
```bash
git add src/organizer/engine.rs src/organizer/mod.rs src/main.rs tests/test_suite/test_organize_all.rs
git commit -m "refactor(organizer): extract run_organize into reusable library engine"
```

---

### Task 3: Implement `run_organize_all` with Mock Subdirectory Suite (`src/organizer/batch.rs`)

**Files:**
- Create: `src/organizer/batch.rs`
- Modify: `src/organizer/mod.rs`
- Modify: `src/main.rs`
- Test: `tests/test_suite/test_organize_all.rs`

**Interfaces:**
- Produces: `pub fn run_organize_all(args: &OrganizeAllArgs) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing unit and mock directory of directories test**
In `tests/test_suite/test_organize_all.rs`, construct a mock directory tree:
- `root/Trip Alpha/` containing sample image(s)
- `root/Trip Beta/` containing sample image(s)
- `root/Empty Trip/`
- `root/birds_hierarchy/` (must be skipped)
- `root/.hidden_dir/` (must be skipped)
- `root/@eaDir/` (must be skipped)
- `root/Test.photoslibrary/` (must be skipped)

Assert that calling `run_organize_all`:
1. Creates `.avicull/manifest.sqlite` and `catalog.html` in `Trip Alpha`, `Trip Beta`, and `Empty Trip`.
2. Leaves `birds_hierarchy`, `.hidden_dir`, `@eaDir`, and `Test.photoslibrary` completely untouched without `.avicull`.
3. Creates `root/catalog.html` and `root/.avicull/manifest.sqlite` containing all bundles from `Trip Alpha` and `Trip Beta`.
4. Re-running `run_organize_all` completes idempotently.

```rust
#[test]
fn test_organize_all_mock_directory_structure() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    
    // Create mock directories
    let trip_a = root.join("Trip Alpha");
    let trip_b = root.join("Trip Beta");
    let empty_trip = root.join("Empty Trip");
    let skipped_hier = root.join("birds_hierarchy");
    let skipped_hidden = root.join(".hidden_trip");
    let skipped_photoslib = root.join("Test.photoslibrary");
    let skipped_eadir = root.join("@eaDir");

    for d in [&trip_a, &trip_b, &empty_trip, &skipped_hier, &skipped_hidden, &skipped_photoslib, &skipped_eadir] {
        std::fs::create_dir_all(d).unwrap();
    }

    // Copy fixture samples into trip_a and trip_b
    std::fs::write(trip_a.join("photo1.jpg"), b"fake_jpeg_1").unwrap();
    std::fs::write(trip_b.join("photo2.jpg"), b"fake_jpeg_2").unwrap();

    let all_args = avicull::cli::OrganizeAllArgs {
        input: root.to_path_buf(),
        output: None,
        no_parent_catalog: false,
        mode: avicull::types::OrganizationMode::InPlace,
        no_prompt: true,
        force: false,
        verbose: true,
        threads: Some(1),
        models_dir: None,
        default_lat: None,
        default_lon: None,
        locations_file: None,
        min_confidence: 0.65,
        min_detect: 0.25,
    };

    avicull::organizer::run_organize_all(&all_args).unwrap();

    // Verify child outputs
    assert!(trip_a.join(".avicull/manifest.sqlite").exists());
    assert!(trip_a.join("catalog.html").exists());
    assert!(trip_b.join(".avicull/manifest.sqlite").exists());
    assert!(trip_b.join("catalog.html").exists());
    assert!(empty_trip.join(".avicull/manifest.sqlite").exists());
    assert!(empty_trip.join("catalog.html").exists());

    // Verify skipped directories were untouched
    assert!(!skipped_hier.join(".avicull").exists());
    assert!(!skipped_hidden.join(".avicull").exists());
    assert!(!skipped_photoslib.join(".avicull").exists());
    assert!(!skipped_eadir.join(".avicull").exists());

    // Verify root rollup catalog
    assert!(root.join(".avicull/manifest.sqlite").exists());
    assert!(root.join("catalog.html").exists());

    // Verify idempotent re-run
    avicull::organizer::run_organize_all(&all_args).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_suite test_organize_all_mock_directory_structure`
Expected: FAIL with `run_organize_all` not found.

- [ ] **Step 3: Implement `run_organize_all` in `src/organizer/batch.rs`**
1. Read entries in `args.input`.
2. Filter directories: skip `.`, `..`, starting with `.`, `birds_hierarchy`, `@eaDir`, ending with `.photoslibrary`.
3. Sort subdirectories alphabetically.
4. For each child directory:
   - Construct child `OrganizeArgs`
   - Call `run_organize(&child_args)`
5. After all children succeed, if `!args.no_parent_catalog`:
   - Construct parent `OrganizeArgs`
   - Call `run_organize(&parent_args)`
6. Connect `Commands::OrganizeAll(all_args) => avicull::organizer::run_organize_all(&all_args)` in `src/main.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_suite test_organize_all_mock_directory_structure`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add src/organizer/batch.rs src/organizer/mod.rs src/main.rs tests/test_suite/test_organize_all.rs
git commit -m "feat(organizer): implement organize-all batch runner with mock directory test"
```

---

### Task 4: CLI Documentation & Help Topics (`src/help.rs`)

**Files:**
- Modify: `src/help.rs:240-270`
- Test: `tests/test_suite/test_organize_all.rs`

**Interfaces:**
- Updates `avicull help` and `avicull --help` outputs with `organize-all`.

- [ ] **Step 1: Write test verifying help output mentions `organize-all`**
Add test in `tests/test_suite/test_organize_all.rs` asserting `avicull::help::render_help(None)` includes `"organize-all"`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test test_suite test_help_includes_organize_all`
Expected: FAIL

- [ ] **Step 3: Add `organize-all` description to `src/help.rs`**
Add `organize-all` under core commands in `src/help.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test test_suite test_help_includes_organize_all`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add src/help.rs tests/test_suite/test_organize_all.rs
git commit -m "docs(help): document organize-all in CLI help reference"
```

---

### Task 5: Full Verification Suite

**Files:**
- None (verification across all crates and scripts)

- [ ] **Step 1: Run format check**
Run: `cargo fmt --check`
Expected: PASS

- [ ] **Step 2: Run strict clippy**
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

- [ ] **Step 3: Run comprehensive test suite**
Run: `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`
Expected: PASS

- [ ] **Step 4: Run rustdoc warning gate**
Run: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
Expected: PASS with 0 warnings
