---
title: "Agentic AI-Native Build-Time Guardrails and Multi-Stage Verification Pipeline"
date: "2026-11-08"
category: "best-practices"
module: "ci"
problem_type: "best_practice"
component: "development_workflow"
severity: "high"
applies_when:
  - "Multi-agent coding workflows generating Rust and Python code"
  - "Preventing production panics and unwrap regressions across automated edits"
  - "Enforcing strict test coverage and documentation standards before merge"
tags:
  - "guardrails"
  - "verification-pipeline"
  - "clippy-denies"
  - "agentic-workflow"
  - "coverage-gates"
---

# Agentic AI-Native Build-Time Guardrails and Multi-Stage Verification Pipeline

## Context
When autonomous AI agents contribute to a complex codebase, they often produce code that superficially appears functional but introduces latent defects:
- Inserting explicit `.unwrap()`, `.expect()`, or `panic!()` calls rather than propagating errors with `?`.
- Generating procedural agent meta-comments (such as `// Step 1: ...`, `// Step 2: Implement...`) that clutter production files.
- Introducing redundant clones or memory allocations on hot processing loops.
- Allowing test coverage to silently regress or breaking Python type assumptions.

Without automated, blocking local guardrails, human developers are forced to manually police code hygiene on every iteration.

## Guidance
`avicull` establishes a 7-stage blocking verification gate executed via `scripts/verify.sh` and backed by workspace compiler lints:

1. **Compiler & Clippy Denials (`Cargo.toml`)**:
   Deny unsafe code, explicit unwraps, and panics across all workspace crates:

   ```toml
   # Cargo.toml
   [lints.rust]
   unsafe_code = "deny"

   [lints.clippy]
   all = { level = "deny", priority = -1 }
   pedantic = { level = "deny", priority = -1 }
   unwrap_used = "deny"
   panic = "deny"
   todo = "deny"
   redundant_clone = "deny"
   ```

2. **Unified Verification Script (`scripts/verify.sh`)**:
   Runs the complete canonical 7-stage pipeline locally:
   - `[1/7]` `cargo fmt --check`: Format compliance.
   - `[2/7]` `cargo check --all-targets`: Compilation without errors.
   - `[3/7]` `cargo clippy --all-targets --all-features -- -D warnings`: Zero-tolerance linting.
   - `[4/7]` `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`: Consolidated integration tests.
   - `[5/7]` `scripts/verify_python.sh`: Python linting (`ruff`), strict typing (`mypy`), and blocking coverage (`pytest-cov` $\ge 80\%$).
   - `[6/7]` `cargo deny check`: Supply-chain security, bans, and license audits against `deny.toml`.
   - `[7/7]` `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: Zero-warning documentation gate.

3. **Dynamic CPU Core Throttling Prevention**:
   `scripts/verify.sh` automatically detects Apple Silicon performance cores (`hw.perflevel0.logicalcpu`) and caps `CARGO_BUILD_JOBS`, preserving efficiency cores for macOS desktop responsiveness.

## Why This Matters
- Prevents production crashes: Eliminating unwrap and panic calls guarantees that corrupt image headers or unexpected filesystem faults propagate graceful errors.
- High velocity: Autonomous agents can iterate and self-correct against `scripts/verify.sh` without requiring human review of trivial lint or typing mistakes.
- Zero license risk: Supply-chain audits ensure all dependencies maintain GPL-3.0-compatible licensing.

## When to Apply
- Before committing any code change or opening a pull request.
- Within CI workflows and pre-push Git hooks.

## Examples
The verification pipeline can be executed in one command:

```bash
./scripts/verify.sh
```

If any stage fails (e.g., an unwrap call or lowered line coverage), the script exits with non-zero status and points directly to the offending file and line.

## Related
- Implemented in `scripts/verify.sh` and `scripts/verify_python.sh`
- Configuration in `Cargo.toml` and `deny.toml`
- Documented in `AGENTS.md` and `CODING_STANDARDS.md`
