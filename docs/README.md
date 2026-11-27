# `avicull` Documentation Index

This directory contains the engineering designs, specifications, implementation plans, execution ledgers, UI brainstorms, and documented solutions for `avicull`.

For user-facing installation and CLI usage, see [README.md](../README.md).
For contributor setup, testing tiers, and release workflows, see [CONTRIBUTING.md](../CONTRIBUTING.md).
For shared domain vocabulary, coding standards, and review policy, see [CONCEPTS.md](../CONCEPTS.md), [CODING_STANDARDS.md](../CODING_STANDARDS.md), and [REVIEW.md](../REVIEW.md).

---

## Directory Structure

| Path | Purpose |
|---|---|
| [`designs/`](designs/) | High-level system architecture and subsystem design documents |
| [`plans/`](plans/) | Standalone implementation and hardening plans |
| [`solutions/`](solutions/) | Categorized post-mortems, bug analyses, and durable architectural learnings |
| [`superpowers/specs/`](superpowers/specs/) | Dated feature design specifications (`YYYY-MM-DD-<feature>-design.md`) |
| [`superpowers/plans/`](superpowers/plans/) | Dated step-by-step implementation plans (`YYYY-MM-DD-<feature>.md`) |
| [`superpowers/sdd/`](superpowers/sdd/) | Spec-Driven Development execution ledgers, task briefs, reports, and review diffs |
| [`superpowers/brainstorm/`](superpowers/brainstorm/) | Interactive HTML UI design explorations and mockups |
| [`avicull_explainer.html`](avicull_explainer.html) | Interactive animated architecture & workflow explainer |

---

## Solutions & Engineering Learnings (`docs/solutions/`)

Each solution document includes YAML frontmatter (`module`, `tags`, `problem_type`) for quick filtering:

- **Architecture Patterns** ([`solutions/architecture-patterns/`](solutions/architecture-patterns/)):
  - Batch subdirectory organization & parent catalog aggregation
  - Domain classification decoupling & multi-category overrides
  - In-place cataloging vs. cosmetic rename decoupling
  - Zero-byte hard-link tree & multi-level coexistence
- **Best Practices** ([`solutions/best-practices/`](solutions/best-practices/)):
  - Agentic AI build-time guardrails & maintainability sensors
- **Logic Errors** ([`solutions/logic-errors/`](solutions/logic-errors/)):
  - Bayesian spatial-temporal prior fusion & detector relevance floors
  - Companion preview path resolution & client eviction remediation
  - Duplicate stem query disambiguation & catalog paths
  - Photographic genres & `_No_Subject_Detected` decoupling
  - Standalone catalog embedded taxonomy fallback & reclassification precedence
  - Universal geolocation parsing & interactive disambiguation
- **Performance Issues** ([`solutions/performance-issues/`](solutions/performance-issues/)):
  - Catalog scroll blanking & two-phase NFS pagination
  - Catalog correction script hang & cached metadata retention
  - Mach-O test harness consolidation on Apple Silicon
  - NFS catalog regeneration & remote NAS parent rollup optimization
- **Runtime Errors** ([`solutions/runtime-errors/`](solutions/runtime-errors/)):
  - macOS FileProvider / iCloud `SF_DATALESS` hydration prevention

---

## SDD Execution Ledgers & UI Brainstorms (`docs/superpowers/`)

### Execution Ledgers (`superpowers/sdd/`)
- [`2026-10-24-avicull/`](superpowers/sdd/2026-10-24-avicull/progress.md) — Core `avicull` pipeline ledger
- [`2026-10-29-catalog-card-display-and-bulk-location/`](superpowers/sdd/2026-10-29-catalog-card-display-and-bulk-location/progress.md) — Card layout & bulk location modal
- [`2026-11-09-ci-speedup/`](superpowers/sdd/2026-11-09-ci-speedup/progress.md) — CI & dev profile optimization
- [`2026-11-10-fix-nfs-inference-and-reclassification/`](superpowers/sdd/2026-11-10-fix-nfs-inference-and-reclassification/progress.md) — Self-contained inference & NFS reclassification
- [`2026-11-11-multi-category-override/`](superpowers/sdd/2026-11-11-multi-category-override/progress.md) — Multi-category override domain refactor
- [`2026-11-17-maintainability-sensors/`](superpowers/sdd/2026-11-17-maintainability-sensors/progress.md) — Complexity, coverage, and contract sensors

### UI Brainstorms (`superpowers/brainstorm/`)
- [`2026-11-11-multi-category-override/`](superpowers/brainstorm/2026-11-11-multi-category-override/interactive-design-spec.html) — Multi-category override UI options & interactive spec
- [`2026-11-18-asset-access-inspector/`](superpowers/brainstorm/2026-11-18-asset-access-inspector/summary-spec.html) — Asset access inspector drawer & high-res access mockups
