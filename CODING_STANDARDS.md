# Coding Standards

Rules governing code written in `avicull`. Reviewers and automated review passes check every diff against these standards.

## Filesystem & Source Immutability

### Source Preservation in Default Mode
In default `--mode in-place`, the input archive is strictly read-only: never move, rename, modify, or unlink any image, RAW, companion, or sidecar file inside `--input`. All state and previews must remain isolated within `<input>/.avicull/` and `<input>/catalog.html`. Neither filename reconciliation nor client-side catalog scripts may execute physical renames on disk in this mode.

### Cosmetic Renaming Opt-In
Renaming source files is purely cosmetic. Only when explicitly requested via `--mode in-place-rename` may files and sidecars be atomically renamed in place. All in-place renames must be transactionally journaled in `.avicull/manifest.sqlite` for lossless rollback via `avicull clean`.

### Zero-Byte Hard Link Parity
Organized output hierarchies on the same APFS volume must share underlying storage inodes using POSIX hard links (`std::fs::hard_link`). Never copy or duplicate raw image payloads. Verify inode parity (`dst_meta.ino() == src_ino`) in filesystem operations.

### Dataless File Protection
Never eagerly open or read files marked with the Darwin `SF_DATALESS` flag (`0x40000000`). Pre-scans must check file flags before opening file descriptors to avoid triggering blocking operating system cloud hydration.

### Traversal Boundary Isolation
Recursive directory discovery must strictly ignore:
- Generated hierarchy folders (`birds_hierarchy`)
- Internal manifest directories (`.avicull`)
- Apple Photos libraries (`*.photoslibrary`)
- Hidden folders (`.*`) and thumbnail caches (`@eaDir`)

### Multi-Level Parent and Child Coexistence
Support simultaneous coexistence of nested trip hierarchies and root hierarchies. Linking operations must handle incrementing inode reference counts without path or collision deadlock.

## Test Harness & Build Architecture

### Single-Binary Test Consolidation
Never create new top-level `tests/test_*.rs` root files for integration tests. Consolidate all test modules under `tests/test_suite/` registered in `tests/test_suite/main.rs`. This prevents Mach-O linker (`ld64`) contention, Apple clang process bloat, and thermal throttling on macOS Apple Silicon.

### Hardware-Conscious Concurrency
Dynamic build job configurations and test execution must respect physical performance cores (`hw.perflevel0.logicalcpu`), leaving efficiency cores free for operating system responsiveness. Never hardcode machine-specific core counts in `.cargo/config.toml`.

## Rust Language & Idiomatic Engineering

### Deny Explicit Panics and Unwraps
Production code must never call `.unwrap()`, `.expect()`, or `panic!()`. Use structured domain error enums (`thiserror`), pattern matching, or `unwrap_or_else` fallbacks.

### Forbid Unsafe Blocks
The codebase enforces `unsafe_code = "deny"`. All memory access, low-level OS bindings, and filesystem operations must use safe abstractions.

### Zero Redundant Clones
Avoid unnecessary heap allocations and clones on hot paths (image hashing, manifest loops, EXIF parsing). Leverage borrowing and string slices (`&str`, `&Path`) across internal boundaries.

### Complete Rustdoc Documentation
All public types, functions, structs, and module roots must carry clear docstrings with `# Arguments`, `# Returns`, and `# Errors` sections. Complex math (Bayesian posteriors, softmax temperatures) must include LaTeX notation.

## Python Data Pipelines & Scripts

### Pure Functions & Immutable Data Models
All mathematical transformations, coordinate calculations, and metadata parsers must be pure functions free of side effects. Data records must use `@dataclass(frozen=True)` or `NamedTuple`.

### Strict Type Rigor & Static Analysis
All Python code must pass `ruff check .`, `ruff format --check .`, and `mypy scripts/` with zero errors.

### Isolated Deterministic Tooling via `uv`
Python tools and test runners must execute via `uv run` for ephemeral, reproducible environment isolation.

## Clean Codebase & Generic Biodiversity

### Zero Agent Meta-Artifacts
Never leave procedural agent comments (such as `// Step 1: ...`, `// Step 2: Implement...`) in committed code. Comments must explain architectural invariants and non-obvious domain logic to human engineers.

### Generic, Data-Driven Biodiverse Logic
Never hardcode personal photographer directories, private trip names, or ad-hoc regional heuristics in production code. All spatial-temporal priors and species checklists must be data-driven, drawn from standardized gazetteers and biodiversity checklists.

## Standalone Distribution & Incremental Manifest Invariants

### Self-Contained Executable Asset Resolution
Never rely on hardcoded paths relative to the current working directory to locate companion scripts or model assets. Binaries executed outside repository workspaces (e.g. on external mounts or system installations) must resolve bundled assets via a fallback ladder: working directory, executable parent path, and embedded asset extraction to standard cache locations (`~/.cache/avicull/`).

### Reclassification Precedence & Incremental Cache Protection
Never unconditionally re-infer unclassified or zero-confidence assets across repeated incremental runs, as non-subject photos legitimately yield zero confidence. Incremental pipelines must enforce strict reclassification precedence: explicit archive-wide CLI reclassification flags (`--reclassify`) force full re-evaluation over cached predictions and prior overrides; normal incremental runs strictly preserve user overrides (including across burst consensus and coordinate updates); coordinate or metadata changes trigger targeted re-inference on non-overridden assets; and unchanged bundles reuse cached manifest decisions without redundant execution.

## Client-Side Catalog UI & Network Mount Responsiveness

### Bounded Pagination & Two-Phase Image Loading
Keep interactive HTML galleries (`src/catalog/template.rs`) bounded via client-side pagination rather than rendering full archives in one flat grid with CSS `content-visibility` virtualization, which causes WebKit/Safari scroll blanking below sticky headers and escapes Chromium-only test suites. On high-latency network mounts, defer full-resolution local image probes until all inline thumbnails on the active page have settled, throttle high-resolution upgrade concurrency, skip non-browser camera RAW formats, and cancel pending probe batches whenever pagination or search filters change. See [catalog-scroll-blanking-and-two-phase-nfs-pagination.md](docs/solutions/performance-issues/catalog-scroll-blanking-and-two-phase-nfs-pagination.md).

### Headless DOM Feature Guards & Fixture Regeneration
Guard optional browser DOM traversal and document lifecycle APIs behind capability checks in client-side catalog scripts so extracted inline `<script>` blocks continue executing cleanly under minimal Node DOM stubs in Rust integration tests. Whenever `src/catalog/template.rs` markup or scripts change, regenerate static test fixtures via the fixture generator example before running Playwright suites.

---

## Maintaining this file

A defect class enters `CODING_STANDARDS.md` only when it appeared in a review finding the author fixed, a fix commit, a cleanup that removed it across the codebase, or a solution record, and no check covers it. State the class as a condition in a few sentences that stay true without naming volatile numbers or symbols, and group rules by area.
