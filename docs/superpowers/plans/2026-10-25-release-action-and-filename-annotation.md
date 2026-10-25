# Release Automation on Merge to Main & Date/Location Filename Augmentation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide automated GitHub releases on every merge to `main`, and augment hard-linked destination filenames with the photo's capture date and inferred geographic location (e.g. `1X7A0791_2025-01-26_Saul_Kere_0.98.jpg`).

**Architecture:** 
1. In `src/types.rs`, augment `AssetBundle` with `resolved_location_name: Option<String>`.
2. In `src/organizer.rs`, update `format_destination_filename` and `materialize_bundle_hardlinks` to extract `YYYY-MM-DD` from `capture_timestamp_ms` and sanitize `resolved_location_name`, assembling `<OriginalStem>_<Date>_<Location>_<Confidence>.<Ext>` with compound sidecar preservation.
3. In `.github/workflows/release_on_merge.yml`, implement a GitHub Actions workflow that detects `Cargo.toml` version, auto-increments patch if the tag already exists, builds release artifacts, and publishes a GitHub Release with tarball and sha256.

**Tech Stack:** Rust 2021, GitHub Actions, APFS hard links, `chrono`, `serde`, `tempfile`.

---

## Global Constraints

- **Strict Source Immutability**: The input archive (`--input`) is strictly read-only. Never modify, move, or unlink files in `--input`.
- **Zero-Byte Output Tree via Hard Links (`std::fs::hard_link`)**: All photo files in `birds_hierarchy` must share underlying inodes with source files (`st_nlink >= 2`).
- **Compound Sidecar Preservation**: `.CR2.xmp`, `.JPG.xmp` must be preserved (e.g. `1X7A0791_2025-01-26_Saul_Kere_0.98.CR2.xmp`).
- **Clean Code Standards**: No procedural agent meta-comments (`// Step 1: ...`).
- **TDD Mandatory**: RED -> GREEN -> REFACTOR for all code changes.

---

## Review Focus

1. **Missing Date or Location Graceful Fallback**: If `capture_timestamp_ms` is `None` or location is unresolved, filename formatting must degrade gracefully without extra dangling underscores (e.g. `1X7A0791_Saul_Kere_0.98.jpg` or `1X7A0791_0.98.jpg`).
2. **Compound Sidecars**: Sidecars matching `DSC_0001.CR2.xmp` must receive identical augmented stems as their primary images so photo management software (Lightroom, Capture One) seamlessly detects them.
3. **Collision Disambiguation**: Cross-trip filename stems that share stem names but have different dates/locations must not collide, and real inode collisions must still trigger deterministic `trip_prefix__` prefixing.
4. **Git Tag Idempotence**: GitHub Action must not crash if a release or tag already exists; it must detect existing tags and bump the patch version cleanly.
5. **APFS Hard Link Invariant**: Destination hardlinks must preserve zero-byte disk footprint.

---

## Tasks

### Task 1: Date & Inferred Location Filename Augmentation

**Files:**
- Modify: `src/types.rs`
- Modify: `src/organizer.rs`
- Modify: `src/main.rs`
- Test: `tests/test_organizer.rs`

- [ ] **Step 1: Write failing unit test in `tests/test_organizer.rs`**
  - Test `format_destination_filename` with date and location tags:
    - stem `1X7A0791`, date `Some("2025-01-26")`, location `Some("Saul_Kere")`, conf `0.98` -> `1X7A0791_2025-01-26_Saul_Kere_0.98.jpg`
    - compound sidecar `1X7A0791.CR2.xmp` -> `1X7A0791_2025-01-26_Saul_Kere_0.98.CR2.xmp`
    - only date `Some("2025-01-26")`, location `None` -> `1X7A0791_2025-01-26_0.98.jpg`
    - only location `Some("Saul_Kere")`, date `None` -> `1X7A0791_Saul_Kere_0.98.jpg`
    - both `None` -> `1X7A0791_0.98.jpg`
  - Verify tests fail.

- [ ] **Step 2: Implement filename formatting and bundle augmentation**
  - In `src/types.rs`: Add `#[serde(default)] pub resolved_location_name: Option<String>` to `AssetBundle`.
  - In `src/organizer.rs`: Update `format_destination_filename` signature and implementation.
  - In `src/organizer.rs`: Update `materialize_bundle_hardlinks` to extract date and sanitize location name.
  - In `src/main.rs`: Attach `bundle.resolved_location_name = Some(loc.name.clone())` when locations are resolved.
  - Verify unit tests in `tests/test_organizer.rs` pass.

- [ ] **Step 3: Update existing integration tests referencing `format_destination_filename`**
  - Run `cargo test` and update any call sites in `tests/test_e2e_real_samples.rs` or `test_spec_conformance.rs`.
  - Ensure all 81 tests pass.

---

### Task 2: Automated GitHub Release Workflow on Merge to Main

**Files:**
- Create: `.github/workflows/release_on_merge.yml`

- [ ] **Step 1: Create `.github/workflows/release_on_merge.yml`**
  - Trigger on `push: branches: [main]`.
  - Steps:
    - Checkout repository with full history (`fetch-depth: 0`).
    - Setup Rust stable.
    - Check current version in `Cargo.toml`.
    - Check if git tag `v$VERSION` exists. If it exists, auto-increment patch version (e.g. `0.2.0` -> `0.2.1`).
    - Build release binary (`cargo build --release`).
    - Create distribution tarball (`dist/avicull-v${RELEASE_VERSION}-aarch64-apple-darwin.tar.gz`) and SHA-256.
    - Create GitHub release using `gh release create` with tag, title, auto-generated release notes, and release assets.
  - Test workflow syntax locally / validate with actionlint or manual review.

---

### Task 3: Real-World Re-Execution on Target Archive & Verification

**Files:**
- Operational execution on `/Users/shaleensharma/Library/CloudStorage/ProtonDrive-mail@shaleen.in/Pictures/pictures_bangalore-2025-`

- [ ] **Step 1: Recompile release binary and re-run `organize`**
  - `cargo build --release`
  - Remove previous `birds_hierarchy` using python `shutil.rmtree`.
  - Run `./target/release/avicull organize --input "/Users/shaleensharma/Library/CloudStorage/ProtonDrive-mail@shaleen.in/Pictures/pictures_bangalore-2025-" --models-dir models --no-prompt`
- [ ] **Step 2: Inspect generated filenames**
  - Verify files have date and location annotations (e.g. `1X7A0791_2025-01-26_Saul_Kere_0.98.jpg`).
  - Verify APFS hard link count (`st_nlink >= 2`).

---

### Task 4: Pull Request Update & Verification

- [ ] **Step 1: Run pre-push hook**
  - Run `bash .githooks/pre-push` to ensure all 7 stages pass cleanly.
- [ ] **Step 2: Commit and push**
  - `git commit -m "feat(organizer): augment filenames with capture date and location, add release-on-merge action"`
  - `git push origin feat/global-gazetteer-location-inference`
- [ ] **Step 3: Update PR #2 on GitHub**
  - Update description with details of filename augmentation and release action.
