# Review Policy

@CODING_STANDARDS.md

This file defines the code review standards and severity definitions for `avicull`. All peer reviews and automated review passes follow this policy.

## Severity Definitions

### Critical
Violations that cause silent data loss, corrupt source archives, panic in production, or compromise filesystem invariants:
- Any modification, deletion, or renaming of files within `--input` (violating Strict Source Immutability).
- Eagerly reading files without checking for Darwin `SF_DATALESS`, risking unhydrated iCloud data download loops.
- Storage bloat caused by copying raw image payloads instead of establishing APFS zero-byte hard links.
- Unhandled `.unwrap()` or panic calls on user input or filesystem errors.

### High
Flaws that break pipeline idempotency, produce invalid catalog states, or fail quality gates:
- Creating separate root `tests/test_*.rs` integration test files instead of registering within `tests/test_suite/main.rs`.
- Inode or sidecar pairing mismatches where metadata sidecars become detached from primary images.
- Unsafe memory operations or compiler warning suppressions bypassing workspace lints.
- Dropping test coverage below blocking thresholds (80% Rust line coverage, 80% Python coverage).

### Medium
Maintainability issues, subtle logic discrepancies, or documentation gaps:
- Mutable Python dictionaries where `@dataclass(frozen=True)` or `NamedTuple` is required.
- Hardcoded regional aliases or heuristic rules that break generic biodiversity handling.
- Missing rustdoc `# Errors`, `# Arguments`, or LaTeX algorithm documentation.
- Procedural agent meta-comments (`// Step 1: ...`, etc.) committed in source files.

### Low / Informational
Stylistic polish, minor performance improvements, or non-blocking suggestions:
- Minor wording improvements in log messages or CLI help text.
- Micro-optimizations on paths outside hot loops.

## What Not to Flag

- Do not flag the absence of third-party UI framework dependencies in `catalog.html`: the catalog is intentionally zero-dependency, self-contained HTML/CSS/JS.
- Do not flag test fixtures in `tests/fixtures/local_user_samples/` for lacking EXIF author tags; metadata has been scrubbed intentionally for privacy and anonymization.
- Do not flag hard links sharing inodes across `birds_hierarchy` directories; multi-level parent and child coexistence is a core architectural invariant.
- Do not flag Python scripts for not using global pip environments; the workspace strictly mandates `uv run` isolation.

## Evidence Requirements for Findings

- A finding must point to a concrete line of code in the diff and cite the exact rule in `CODING_STANDARDS.md` or architectural invariant violated.
- Performance degradation claims must reference concrete bottlenecks (e.g. Mach-O linker contention or hot-path allocations).
- Safety issues (such as dataless hydration or source mutability) must trace the execution path from entry point to uninspected file operation.
