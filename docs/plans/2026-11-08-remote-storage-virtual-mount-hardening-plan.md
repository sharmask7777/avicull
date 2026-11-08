# Plan: Complete Remote Storage Streaming & WebDAV Virtual Mount

**Goal**: Transform the prototype remote storage virtual mount into a robust, high-performance, safe, and fully functional macOS virtual drive projection for local and S3 photo archives.

---

## Architecture Decomposition & Files

- `src/storage/preview_extractor.rs`:
  - `MAX_PREVIEW_SIZE_BYTES = 64 * 1024 * 1024` cap to prevent OOM crashes.
  - Safe bounds arithmetic (`checked_add`).
  - SubIFD traversal (`0x014A`) to extract embedded JPEGs from real Canon CR2, Nikon NEF, and Sony ARW files.
  - JPEG SOI magic byte verification (`0xFFD8`).
- `src/storage/opendal_backend.rs`:
  - Secret sanitization covering AWS query parameters (`X-Amz-Signature`, `AWSAccessKeyId`), session tokens, and Bearer tokens.
  - S3 configuration supporting AWS profile, region, and custom endpoints.
- `src/storage/cache.rs` (new):
  - Local disk preview cache storing extracted JPEG previews keyed by source URI hash or ETag.
  - Concurrency throttle via `tokio::sync::Semaphore(16)`.
- `src/storage/webdav_server.rs`:
  - Full `dav_server::fs::DavFileSystem` implementation (`AvicullDavFs`).
  - Strict read-only enforcement (`FsError::Forbidden` on write/delete/create).
  - Synthetic `/catalog.html` virtual file projection.
- `src/storage/mount_service.rs` (new):
  - Loopback HTTP server running on 2-thread Tokio runtime.
  - Ephemeral Basic Auth token generation.
  - Darwin `/sbin/mount_webdav` process invocation.
  - Signal trap (`SIGINT`/`SIGTERM`) with synchronous `diskutil unmount force` cleanup.
- `src/cli.rs` & `src/main.rs`:
  - Add `--profile`, `--region`, `--endpoint`, `--open` flags.
  - Wire `execute_mount` and `execute_unmount` to `mount_service`.
- `src/help.rs`:
  - Add `HelpTopic::Mount` with complete usage instructions and examples.

---

## Detailed Tasks

### Task 1: Safety & Parsing Hardening (`PreviewExtractor` & `sanitize_uri`)
- **TDD RED**: Write tests in `tests/test_suite/test_preview_extractor.rs`:
  - Test rejecting preview lengths $> 64$ MB with an error instead of allocating.
  - Test SubIFD parsing on TIFF structures with tag `0x014A`.
  - Test corrupt header bounds and integer overflow protection.
  - Test `sanitize_uri` scrubbing AWS query parameters (`?AWSAccessKeyId=...&Signature=...`).
- **GREEN**:
  - Update `src/storage/preview_extractor.rs` with bounds safety, SubIFD traversal, and size cap.
  - Update `src/storage/opendal_backend.rs` with regex/query-param scrubber.
- **VERIFY**: Run `cargo test --test test_suite test_preview_extractor test_remote_opendal`.

### Task 2: Persistent Preview Cache & Remote Concurrency Throttling
- **TDD RED**: Write tests in `tests/test_suite/test_remote_cache.rs`:
  - Verify disk cache hit avoids calling underlying backend.
  - Verify semaphore permits throttle concurrent fetches to maximum 16.
- **GREEN**:
  - Implement `DiskPreviewCache` in `src/storage/cache.rs`.
  - Wire into `PreviewExtractor`.
- **VERIFY**: Run `cargo test --test test_suite test_remote_cache`.

### Task 3: Full `dav_server::fs::DavFileSystem` Implementation
- **TDD RED**: Write tests in `tests/test_suite/test_dav_fs.rs`:
  - Test `DavFileSystem::open`, `DavFileSystem::read_dir`, `DavFileSystem::metadata`.
  - Test write rejection: verify `create_dir`, `remove_file`, `write` return `FsError::Forbidden`.
  - Test synthetic `/catalog.html` virtual file access.
- **GREEN**:
  - Implement `AvicullDavFs` and associated `DavFile` / `DavDir` types in `src/storage/webdav_server.rs`.
- **VERIFY**: Run `cargo test --test test_suite test_dav_fs`.

### Task 4 & 5: Loopback Service, Ephemeral Auth, Signal Handling & Mount Lifecycle
- **TDD RED**: Write tests in `tests/test_suite/test_mount_lifecycle.rs`:
  - Test loopback server binding with ephemeral token.
  - Test unauthorized requests return 401.
  - Test mount point teardown logic and diskutil unmount invocation.
- **GREEN**:
  - Implement `src/storage/mount_service.rs` with 2-worker Tokio runtime, HTTP Basic Auth, and signal traps.
  - Wire `execute_mount` and `execute_unmount` in `src/main.rs`.
- **VERIFY**: Run `cargo test --test test_suite test_mount_lifecycle`.

### Task 6: CLI Options, Help System & Verification Pipeline
- **TDD RED**: Write tests in `tests/test_suite/test_help.rs` for `HelpTopic::Mount`.
- **GREEN**:
  - Add `--profile`, `--region`, `--endpoint`, `--open` in `src/cli.rs`.
  - Add `HelpTopic::Mount` in `src/help.rs` and register in `HelpTopic::All`.
  - Run `./scripts/verify.sh` for complete 7-stage gate (formatting, check, clippy, tests, python, deny, doc).
