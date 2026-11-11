---
title: "Eliminating Catalog Scroll Blanking and NFS Stalls via Bounded Pagination and Two-Phase Rendering"
date: "2026-11-11"
category: "performance-issues"
module: "catalog"
problem_type: "performance_issue"
component: "frontend"
severity: "high"
symptoms:
  - "Scrolling catalog.html in WebKit/Safari below the sticky header renders the entire card grid blank"
  - "Opening ~1,000-photo archives over NFS stalls gallery image loading for tens of seconds per row and freezes typing in search"
  - "High-resolution local JPEG and unrenderable camera RAW probes saturate browser I/O before inline micro-thumbnails finish decoding"
  - "Adding DOM traversal helpers without capability checks breaks inline JS execution under minimal Node test stubs"
root_cause: "wrong_api"
resolution_type: "code_fix"
tags:
  - "catalog"
  - "nfs-performance"
  - "webkit"
  - "content-visibility"
  - "pagination"
  - "thumbnail-first"
  - "two-phase-loading"
  - "playwright"
---

# Eliminating Catalog Scroll Blanking and NFS Stalls via Bounded Pagination and Two-Phase Rendering

## Problem
Opening a standalone generated `catalog.html` (15.4 MB HTML covering 987 bundles, each backed by ~17 MB processed JPEGs or camera RAW files on an NFS share mounted under `/Volumes/...`) caused severe UI and scrolling failures:
1. Typing into search froze the main thread synchronously rebuilding the DOM for all ~987 cards per keystroke, while opening the page took tens of seconds per row to display usable imagery.
2. An initial CSS `content-visibility: auto` workaround passed headless Chromium Playwright tests, yet caused the entire grid below the sticky `<header>` to go completely blank upon scrolling in macOS Safari/WebKit.

## Symptoms
- **WebKit/Safari scroll blanking**: On initial load above the fold, cards rendered below the sticky header (`position: sticky; top: 0`); immediately upon scrolling, the gallery area painted solid dark background with zero visible cards.
- **Per-row NFS image stalls**: Visible cards stayed blank for tens of seconds because full-resolution ~17 MB JPEGs (and unrenderable `.CR2` RAW files) saturated `file://` NFS client I/O and browser image-decoder queues ahead of or concurrently with inline micro-thumbnail decoding.
- **Test harness traps during iteration**: Adding `grid.querySelectorAll(...)` or `document.body.contains(...)` without capability checks passed Playwright Chromium tests while breaking bare `node` DOM simulations inside `cargo test`, and un-regenerated static fixtures (`cargo run --example generate_fixture_catalog`) caused Playwright assertions to run against stale HTML.

## What Didn't Work
- **CSS `content-visibility: auto` on `.card` inside responsive CSS Grid**: Adding `content-visibility: auto` and `contain-intrinsic-size` while rendering all ~987 cards in one flat grid caused WebKit/Safari compositor layer blanking below the sticky header during scroll. Because `tests/e2e/playwright.config.ts` runs only Chromium (`Desktop Chrome`), automated E2E passed without catching the Safari regression.
- **Unbounded grid + eager `IntersectionObserver` `attemptHighResSwap()` probes**: Rendering all items at once and probing full-res `local_src` on intersection (without waiting for active thumbnails or skipping RAW formats) exhausted NFS I/O and image-decoder capacity, delaying basic grid previews. Even clicking into modals triggered futile browser loads on camera RAW paths until `isRawFormat` checks were added.

## Solution
Fixed in PR `#58` (`src/catalog/template.rs`) and codified in `CODING_STANDARDS.md` under `Client-Side Catalog UI & Network Mount Responsiveness`:

1. **Bounded Client-Side Pagination Instead of `content-visibility: auto`**:
   - Removed `content-visibility: auto` from `.card` CSS rules (`src/catalog/template.rs:411-419`).
   - Added bounded pagination (`let pageSize = 60;` at `src/catalog/template.rs:1474`) with `#paginationBar` controls (`src/catalog/template.rs:1193-1209`, pagination CSS rules at `src/catalog/template.rs:695-804`) and automatic reset (`currentPage = 1`) on search (`src/catalog/template.rs:2103`) or category/filter transitions, bounding rendered grid cards to 60 items by default while preserving `All` opt-in.

2. **Strict Two-Phase Thumbnail-First Loading with Bounded Concurrency & Batch Cancellation**:
   - **Phase 1 (Thumbnails First)**: `renderGallery()` increments a monotonic token (`const currentToken = ++thumbnailBatchToken;` at `src/catalog/template.rs:1986`), renders only the active page's cards preferring inline base64 micro-thumbnails (`embedded_thumb_data_uri` at `src/catalog/template.rs:1941-1947`, using `loading="lazy" decoding="async"` at `1962`), and attaches `load`/`error` listeners (checking `img.complete` for already-decoded data URIs) across `.card-media img` (`src/catalog/template.rs:1993-2024`).
   - **Phase 2 (Deferred High-Res Upgrades)**: Only once all active thumbnails settle—and only if `token === thumbnailBatchToken`—does `onAllThumbnailsRendered(token, pageItems)` enqueue high-res local probes drained by `MAX_HIGH_RES_CONCURRENCY = 2` workers (`src/catalog/template.rs:1478, 1831-1882`). Non-empty page or search renders bump `thumbnailBatchToken` at `src/catalog/template.rs:1986` to cancel stale queues (note: zero-match searches return early at `1892-1896` before bumping the token, and `document.body.contains(job.img)` at `1861-1864` is checked inside `probe.onload`, so queued NFS probes finish in the background without mutating detached DOM until a non-empty render cancels them).

3. **Camera RAW Format Skip & Search Indexing**:
   - Defined `RAW_EXTENSIONS` and `isRawFormat(path)` (`src/catalog/template.rs:1480-1489`) to skip browser-unrenderable camera RAW formats (`.cr2`, `.cr3`, `.nef`, `.arw`, `.dng`, `.orf`, `.rw2`, `.pef`, `.raf`, `.sr2`, `.srf`) at active runtime callers `onAllThumbnailsRendered` (`src/catalog/template.rs:1841`) and `openModal` (`src/catalog/template.rs:2172`), while keeping legacy `attemptHighResSwap` (`1558`) guarded for test symbol compatibility (`tests/test_suite/test_catalog.rs:2572`).
   - Precomputed normalized `item._searchIndex` (`src/catalog/template.rs:1492-1503`) and debounced `handleSearch` (`100` ms `searchDebounceTimer` at `src/catalog/template.rs:2094-2111`) so filtering skips per-field allocation churn and avoids intermediate DOM rebuilds while typing.

4. **Minimal-DOM Node Stub Safety Guards**:
   - Guarded `.card-media img` queries and detached-element checks with `typeof grid.querySelectorAll === 'function'` (`src/catalog/template.rs:1993-1994`) and `document.body && typeof document.body.contains === 'function'` (`src/catalog/template.rs:1861-1862`), keeping `tests/test_suite/test_catalog.rs:2518` (`test_catalog_standalone_dynamic_high_res_replacement`) green without brittle mock expansion.

## Why This Works
- Eliminating `content-visibility: auto` while slicing active DOM cards (`pageSize = 60`, `1903-1904`) removes both large-DOM scroll/filter layout cost and WebKit's CSS Grid `content-visibility` compositor blanking. Note that choosing `All` (`1206`) disables pagination, re-exposing $O(N)$ DOM creation and queuing up to $N$ high-res NFS upgrades.
- Sequencing high-res probes behind active-page thumbnail settlement (`onAllThumbnailsRendered`) with `MAX_HIGH_RES_CONCURRENCY = 2`, monotonic `thumbnailBatchToken` cancellation, and `isRawFormat` filtering ensures visible cards paint instant inline micro-thumbnails before issuing multi-megabyte `file://` NFS reads or impossible RAW decodes. Caveats: if an item lacks an inline `embedded_thumb_data_uri` and falls back to `thumb_src` below the fold, `loading="lazy"` (`1962`) defers that image until scrolled (holding `settledCount < totalToSettle` until scrolled); also `probe.onload` fires on resource load rather than `probe.decode()`, so swapping `job.img.src` can still trigger async image decoding.
- Precomputing `_searchIndex` keeps `getFilteredItems()` (`1627-1641`) an allocation-light $O(N)$ scan while pagination bounds DOM rebuilds (`grid.innerHTML`) to $O(\min(\text{pageSize}, \text{matches}))$. Scaling ceiling: because `CATALOG_DATA` (`1456`) embeds `300x300` quality-68 JPEG data URIs inline (`src/catalog/mod.rs:832-839`, totaling 15.4 MB for 987 bundles), multi-10k archives (`>10,000` items) will scale HTML size and initial `JSON.parse` memory into hundreds of megabytes regardless of pagination.

## Prevention
- **Verification Scope**: Automated tests verify CSS exclusions, script structural invariants, bare Node DOM mock execution, and Chromium pagination/initial rendering; WebKit compositor scrolling and live NFS load latencies are validated manually against real mounts rather than executed in CI.
- **Standards Rule**: `CODING_STANDARDS.md` (`Client-Side Catalog UI & Network Mount Responsiveness`) captures the architectural rules prohibiting CSS `content-visibility` virtualization over Grid, requiring two-phase thumbnail-first loading + cancellation + RAW skips, and requiring headless DOM feature guards + fixture regeneration.
- **Rust Unit & Subprocess Guards (`tests/test_suite/test_catalog.rs`)**:
  - `test_catalog_nfs_responsiveness_and_search_optimizations` (`tests/test_suite/test_catalog.rs:3842-3912`) explicitly asserts `!html.contains("content-visibility: auto")` (`3887`) alongside pagination controls (`3877-3885`) and two-phase settlement gating (`3908-3911`).
  - `test_catalog_standalone_dynamic_high_res_replacement` (`tests/test_suite/test_catalog.rs:2518`) executes extracted JS in bare `node` with a minimal DOM stub, immediately catching unguarded browser DOM assumptions.
- **Playwright E2E Verification (`tests/e2e/bird_catalog.spec.ts:513-578`)**: `'Pagination & Thumbnail-First Rendering'` verifies pagination bounds, search reset to page 1, and non-RAW initial `img[src]` rendering in Chromium (while two-phase settlement gating itself is asserted in `tests/test_suite/test_catalog.rs:3908-3911`); after editing `src/catalog/template.rs`, regenerate fixtures via `cargo run --example generate_fixture_catalog` and manually verify scroll behavior in macOS Safari/WebKit when changing gallery CSS or sticky header layers (or extend `tests/e2e/playwright.config.ts` if WebKit binaries are provisioned).

## Related Issues
- Fixed and merged in PR `#58` (`https://github.com/sharmask7777/avicull/pull/58`).
- Complements `docs/solutions/performance-issues/nfs-catalog-regeneration-optimization.md` (server/CLI `catalog.html` generation over NFS in `src/catalog/mod.rs`, including `is_local` preview handling).
- Complements `docs/solutions/performance-issues/catalog-correction-script-hang-and-cached-metadata-retention.md` (`template.rs` batch/correction shell script NFS hang elimination).
- Relates to `docs/solutions/logic-errors/standalone-catalog-embedded-taxonomy-fallback.md` (Node subprocess + Playwright testing patterns for `template.rs`) and `docs/solutions/runtime-errors/macos-fileprovider-dataless-hydration-prevention.md` (Darwin `SF_DATALESS` cloud-eviction detection in `src/geo.rs` and `src/storage/local.rs`).
