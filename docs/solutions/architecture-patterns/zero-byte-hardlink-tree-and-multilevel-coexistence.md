---
title: "Zero-Byte Hardlink Tree Organization and Multi-Level Parent-Child Coexistence"
date: "2026-11-08"
category: "architecture-patterns"
module: "organizer"
problem_type: "architecture_pattern"
component: "infrastructure"
severity: "critical"
applies_when:
  - "Organizing large image archives without consuming additional storage on APFS"
  - "Running hierarchical organization across nested subdirectories and parent folders"
  - "Ensuring read-only immutability of input photo collections"
tags:
  - "apfs"
  - "hard-links"
  - "source-immutability"
  - "zero-byte-storage"
---

# Zero-Byte Hardlink Tree Organization and Multi-Level Parent-Child Coexistence

## Context
Wildlife photographers frequently maintain tens of thousands of RAW camera files (`.CR2`, `.NEF`, `.ARW`) and high-resolution JPEGs totaling hundreds of gigabytes or terabytes. Traditional photo organization tools either:
1. Move or rename files within the source directory, mutating the photographer's original archive and breaking references in editing tools (Lightroom, Capture One).
2. Copy files to a new organized hierarchy, doubling disk space consumption and taking hours of disk I/O.
3. Fall into infinite recursion loops when a parent directory is organized after a child trip folder has already materialized an organized sub-hierarchy.

## Guidance
`avicull` guarantees two structural invariants: **Strict Source Immutability** and **Zero-Byte Output Trees via POSIX Hard Links (`std::fs::hard_link`)**:

1. **Inode Parity (`dst_meta.ino() == src_ino`)**:
   When organizing an input archive into `<output>/birds_hierarchy`, files are linked rather than copied. The organized file points to the identical physical filesystem inode:

   ```rust
   // src/organizer/mod.rs
   fs::hard_link(&src_path, &dst_path).with_context(|| {
       format!(
           "Failed to create hard link from '{}' to '{}'",
           src_path.display(),
           dst_path.display()
       )
   })?;
   ```

2. **Multi-Level Coexistence**:
   A photographer can run `avicull` on a specific trip folder (e.g., `Pictures/Leh`), materializing `Pictures/Leh/birds_hierarchy` (incrementing file link count `st_nlink` from 1 to 2). Later, they can run `avicull` across the entire parent directory `Pictures`, materializing `Pictures/birds_hierarchy` (incrementing `st_nlink` from 2 to 3). Both trees co-exist simultaneously without duplicate storage consumption.

3. **Traversal Skip Rules**:
   To prevent recursive loops and scanning generated links as new assets, `src/scanner.rs` strictly excludes:
   - Any directory named `birds_hierarchy`
   - Internal database metadata directories (`.avicull`)
   - Apple Photos libraries (`*.photoslibrary`)
   - Hidden system folders (`.*`) and thumbnail directories (`@eaDir`)

## Why This Matters
- Zero storage expansion: A 500 GB RAW photo archive generates an organized taxonomy tree consuming 0 additional disk blocks for image payloads.
- Safety: The source archive is opened purely read-only; no original file is ever renamed, deleted, or unlinked.
- Composability: Photographers can view their photos chronologically by trip, or taxonomically by species, without choosing between them.

## When to Apply
- Whenever generating organized output hierarchies on APFS or POSIX-compliant filesystems.
- Whenever designing CLI commands that create alternate filesystem views of user data.

## Examples
Verified in `tests/test_suite/test_spec_conformance.rs` through `test_nested_and_parent_coexistence_via_hardlinks`:

```rust
// 1. Organize child trip
let src_meta = fs::metadata(&photo1).unwrap();
assert_eq!(src_meta.nlink(), 1);

// Materialize Leh trip hierarchy:
materialize_bundle_hardlinks(&leh_bundles[0], "run-leh-1", &leh_hierarchy, false).unwrap();
assert_eq!(fs::metadata(&photo1).unwrap().nlink(), 2);

// 2. Organize parent directory:
materialize_bundle_hardlinks(&parent_bundles[0], "run-parent-1", &parent_hierarchy, false).unwrap();
assert_eq!(fs::metadata(&photo1).unwrap().nlink(), 3);
```

## Related
- Verified by `tests/test_suite/test_spec_conformance.rs`
- Invariant documented in `AGENTS.md` and `src/organizer/mod.rs`
