# Implementation Plan: Remote & Cloud Storage Streaming Engine with Native Virtual Mount

## Implementation plan

### Step 1: Apache OpenDAL Integration & Storage Engine Abstraction
- Add `opendal` dependency to `Cargo.toml` with `services-s3` feature.
- Implement `OpenDalBackend` implementing the existing `StorageBackend` trait in `src/storage/opendal_backend.rs`.
- Support S3/MinIO and local file endpoints via OpenDAL operator configuration.

### Step 2: Two-Request Container-Aware Embedded Preview Extractor
- Implement asynchronous byte-range TIFF/EXIF reader in `src/storage/preview_extractor.rs`.
- Request 1: Fetch initial 128 KB header (`Range: bytes=0-131071`) and parse TIFF IFD tags to extract embedded JPEG preview offset & length.
- Request 2: Fetch exact byte range for preview payload directly into memory buffer.
- Add unit tests verifying byte efficiency on mock Canon CR2, Nikon NEF, and Sony ARW containers.

### Step 3: Taxonomic WebDAV Loopback Projection Engine
- Integrate `dav-server` for a lightweight embedded WebDAV filesystem in `src/storage/webdav_server.rs`.
- Map SQLite classification database into dynamic taxonomic paths: `/Aves/<Order>/<Family>/<Species>/<filename>`.
- Intercept virtual file reads: stream embedded preview or proxy on-demand RAW range queries.

### Step 4: CLI Command & Native macOS Finder Mounting
- Add `mount` subcommand to `src/cli.rs`: `avicull mount <SOURCE_URI> <MOUNT_POINT>`.
- In `src/storage/mod.rs` & `src/main.rs`, spawn the local WebDAV loopback server on an ephemeral port.
- Issue macOS `mount_webdav -i http://127.0.0.1:<PORT>/ <MOUNT_POINT>` to seamlessly expose the taxonomic hierarchy in Finder.
- Ensure graceful teardown (unmount on SIGINT/Ctrl+C).

### Step 5: Verification & End-to-End Testing
- Write integration tests in `tests/test_suite/` verifying zero remote mutations, byte-range efficiency, and WebDAV tree correctness.
- Run `scripts/verify.sh` to ensure zero clippy warnings, green tests, and doc integrity.


<!-- autoplan-accepted:ceo -->
- Dynamic range parser: Implement adaptive byte-range TIFF parser starting with 128 KB heuristic window and dynamically chasing SubIFD/MakerNote pointers if located beyond the initial buffer.
- WebDAV mount safety & teardown guard: Implement strict SIGINT/SIGTERM process teardown with `diskutil unmount` fallback and fast 404 responses for macOS metadata probes (`.DS_Store`, `._*`).
- Persistent local preview cache: Cache extracted JPEG previews locally in SQLite or `.avicull/cache/` keyed by S3 ETag/object version to eliminate redundant egress costs.
- Storage capability typing: Explicitly distinguish between `LocalPosixStorage` (APFS hardlink capable) and `RemoteObjectStorage` (virtual projection & stream capable) to honor AGENTS.md invariant constraints.
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:dx -->
- CLI Ergonomics & Symmetric Unmount: Provide optional `[MOUNT_POINT]` defaulting to `/Volumes/avicull_birds`, add `--serve-only` / `--port <PORT>` escape hatches, and implement a dedicated `avicull unmount <MOUNT_POINT>` command.
- Non-Interactive Mount Execution: Remove `-i` flag from `mount_webdav` to eliminate terminal interactive hangs in automated/agent runs.
- Credential Redaction: Sanitize all OpenDAL error channels and diagnostics to guarantee zero leakage of S3 Access Keys or Secret Tokens.
- Agent-Ready Error Contracts: Implement stable error codes (`ERR_MOUNT_TARGET_OCCUPIED`, `ERR_MANIFEST_NOT_FOUND`) with actionable recovery fixes on stdout/stderr.
- Local Mock Evaluation: Enable `services-fs` in OpenDAL so developers can test the virtual mount engine on local folders without S3 credentials.
<!-- /autoplan-accepted:dx -->

<!-- autoplan-accepted:eng -->
- Storage Trait Partitioning: Introduce `RemoteObjectBackend` separate from `PosixStorageBackend` so remote cloud storage never exposes invalid mutating POSIX filesystem operations (rename, hard_link, Darwin eviction flags).
- Multi-Container RAW Parser: Verify container magic bytes (TIFF, ISOBMFF for Canon CR3, RAF) and enforce an upper bound of 3 targeted byte-range requests before falling back to full streaming.
- Loopback Authentication & Security: Generate ephemeral session credentials on loopback WebDAV mount to prevent localhost port sniffing and DNS rebinding exfiltration.
- In-Memory Taxonomic Prefix Tree: Pre-index SQLite classification metadata into an in-memory `Arc<TaxonomicTree>` to ensure Finder `PROPFIND` requests return in O(1) time without database lock contention.
- macOS webdavfs Quirks & Metadata Handlers: Support dummy 200/204 responses for `LOCK`/`PROPPATCH`/`PUT` on macOS metadata artifacts (`._*`, `.DS_Store`) to prevent Finder error popups.
- Tokio Runtime Concurrency Guard: Explicitly restrict the async Tokio worker threads to 2 cores, preserving Apple Silicon performance cores for Rayon and Metal ML pipelines.
<!-- /autoplan-accepted:eng -->
## Review record

<!-- autoplan-accepted:ceo -->
- Dynamic range parser: Implement adaptive byte-range TIFF parser starting with 128 KB heuristic window and dynamically chasing SubIFD/MakerNote pointers if located beyond the initial buffer.
- WebDAV mount safety & teardown guard: Implement strict SIGINT/SIGTERM process teardown with `diskutil unmount` fallback and fast 404 responses for macOS metadata probes (`.DS_Store`, `._*`).
- Persistent local preview cache: Cache extracted JPEG previews locally in SQLite or `.avicull/cache/` keyed by S3 ETag/object version to eliminate redundant egress costs.
- Storage capability typing: Explicitly distinguish between `LocalPosixStorage` (APFS hardlink capable) and `RemoteObjectStorage` (virtual projection & stream capable) to honor AGENTS.md invariant constraints.
<!-- /autoplan-accepted:ceo -->

<!-- autoplan-accepted:dx -->
- CLI Ergonomics & Symmetric Unmount: Provide optional `[MOUNT_POINT]` defaulting to `/Volumes/avicull_birds`, add `--serve-only` / `--port <PORT>` escape hatches, and implement a dedicated `avicull unmount <MOUNT_POINT>` command.
- Non-Interactive Mount Execution: Remove `-i` flag from `mount_webdav` to eliminate terminal interactive hangs in automated/agent runs.
- Credential Redaction: Sanitize all OpenDAL error channels and diagnostics to guarantee zero leakage of S3 Access Keys or Secret Tokens.
- Agent-Ready Error Contracts: Implement stable error codes (`ERR_MOUNT_TARGET_OCCUPIED`, `ERR_MANIFEST_NOT_FOUND`) with actionable recovery fixes on stdout/stderr.
- Local Mock Evaluation: Enable `services-fs` in OpenDAL so developers can test the virtual mount engine on local folders without S3 credentials.
<!-- /autoplan-accepted:dx -->

<!-- autoplan-accepted:eng -->
- Storage Trait Partitioning: Introduce `RemoteObjectBackend` separate from `PosixStorageBackend` so remote cloud storage never exposes invalid mutating POSIX filesystem operations (rename, hard_link, Darwin eviction flags).
- Multi-Container RAW Parser: Verify container magic bytes (TIFF, ISOBMFF for Canon CR3, RAF) and enforce an upper bound of 3 targeted byte-range requests before falling back to full streaming.
- Loopback Authentication & Security: Generate ephemeral session credentials on loopback WebDAV mount to prevent localhost port sniffing and DNS rebinding exfiltration.
- In-Memory Taxonomic Prefix Tree: Pre-index SQLite classification metadata into an in-memory `Arc<TaxonomicTree>` to ensure Finder `PROPFIND` requests return in O(1) time without database lock contention.
- macOS webdavfs Quirks & Metadata Handlers: Support dummy 200/204 responses for `LOCK`/`PROPPATCH`/`PUT` on macOS metadata artifacts (`._*`, `.DS_Store`) to prevent Finder error popups.
- Tokio Runtime Concurrency Guard: Explicitly restrict the async Tokio worker threads to 2 cores, preserving Apple Silicon performance cores for Rayon and Metal ML pipelines.
<!-- /autoplan-accepted:eng -->
