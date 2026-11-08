---
title: "Preventing Unwanted macOS iCloud FileProvider Cloud Hydration via SF_DATALESS Inspection"
date: "2026-11-08"
category: "runtime-errors"
module: "storage"
problem_type: "runtime_error"
component: "infrastructure"
severity: "critical"
symptoms:
  - "Scanning cloud-synced photo archives (iCloud, Dropbox, Proton Drive) triggers massive background downloads"
  - "Local disk space suddenly exhausted during photo archive discovery"
  - "CLI processes hanging on file open calls while waiting on network file hydration"
root_cause: "wrong_api"
resolution_type: "code_fix"
tags:
  - "macos"
  - "fileprovider"
  - "dataless-faults"
  - "icloud"
  - "apfs"
---

# Preventing Unwanted macOS iCloud FileProvider Cloud Hydration via SF_DATALESS Inspection

## Problem
On macOS, cloud storage providers integrating with the Apple FileProvider framework (iCloud Drive, Dropbox, Proton Drive, Google Drive) represent files stored in the cloud as local directory stubs with 0 disk blocks allocated. If an application calls standard filesystem read APIs (such as `std::fs::File::open` or `std::fs::read`), the macOS kernel halts the process on a "dataless fault" and forces the FileProvider daemon to download the full multi-megabyte image over the network before the system call returns.

When `avicull` scans an archive containing thousands of cloud-backed RAW photos, eager file inspection triggers uncontrollable multi-gigabyte downloads, exhausting bandwidth and disk space, and freezing scanner threads.

## Symptoms
- Traversing a synced directory triggers macOS notifications that files are being downloaded from iCloud or cloud providers.
- Scanner worker threads stall indefinitely waiting for network downloads.
- Local SSD storage rapidly fills up during initial metadata pre-scans.

## What Didn't Work
- Checking file length (`meta.len() > 0`) does not work: macOS FileProvider reports the logical file size (e.g. 25 MB for a RAW image) even when zero physical blocks reside on local disk.
- Checking physical block allocation counts (`st_blocks == 0`) is unreliable across network mounts and sparse APFS files.
- Catching I/O timeouts fails because FileProvider hydration blocks synchronously in the kernel VFS layer before returning control to user space.

## Solution
Inspect BSD file flags using `std::os::darwin::fs::MetadataExt::st_flags` on `symlink_metadata` before opening any file. Darwin defines the `SF_DATALESS` flag as `0x40000000`.

In `src/geo.rs` and `src/storage/local.rs`:

```rust
// src/geo.rs
pub fn is_dataless_file(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::darwin::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x4000_0000;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            return (meta.st_flags() & SF_DATALESS) != 0;
        }
    }
    let _ = path;
    false
}
```

`symlink_metadata` queries inode attributes without triggering dataless faults. If `is_dataless_file` returns `true`, the scanner bypasses eager EXIF parsing and neural classification for unhydrated files, or reports them to the manifest as cloud-pending stubs.

## Why This Works
`symlink_metadata` invokes the Darwin `lstat64` system call, which retrieves inode flags directly from the filesystem catalog without opening a file descriptor or faulting in data pages. Bitwise testing `st_flags & 0x40000000` immediately identifies cloud-managed stubs with zero network I/O.

## Prevention
- `AGENTS.md` Invariant 6 mandates: "Files marked with Darwin `SF_DATALESS` (`0x40000000`) must never be opened eagerly during pre-scans to prevent unwanted on-demand cloud hydration."
- `CODING_STANDARDS.md` requires that all file scanners test locality via `is_dataless_file` before attempting content reads.

## Related Issues
- Implemented in `src/geo.rs` and `src/storage/local.rs`
- Invariant documented in `AGENTS.md`
