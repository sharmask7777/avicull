# Contributing to `avicull`

Thank you for your interest in contributing to `avicull`!

`avicull` is a high-performance, zero-byte avian photo culling and hierarchy organization tool tailored for macOS Apple Silicon. To maintain its strict safety guarantees, zero-byte APFS hardlink invariants, and exceptional code quality, we follow a rigorous engineering workflow.

Please read this guide before submitting issues, pull requests, or code contributions.

---

## 1. Architectural Invariants (Non-Negotiable)

Every contributor and AI agent must adhere strictly to these core invariants:

1. **Source Immutability**:
   The input photo archive (`--input`) is strictly read-only. Never move, rename, delete, or modify any file in `--input` during hierarchy generation.
2. **Zero-Byte Output Tree via APFS Hard Links**:
   The generated `birds_hierarchy/` must reference original files using POSIX hard links (`std::fs::hard_link`), asserting `dst_meta.ino() == src_ino`. 0 additional disk bytes must be consumed for image payloads.
3. **Traverse Exclusions**:
   Directory scans must skip `birds_hierarchy`, `.avicull`, `*.photoslibrary`, `@eaDir`, and hidden directories (`.*`).
4. **Parent & Child Multi-Level Coexistence**:
   Trip subdirectory hierarchies and parent output directories must coexist simultaneously without collision.
5. **Dataless File Protection**:
   Files marked with Darwin `SF_DATALESS` (`0x40000000`) in macOS FileProvider / iCloud Drive must never be opened eagerly during pre-scans.
6. **No Agent Meta-Artifacts**:
   Never check in procedural AI planning comments (e.g. `// Step 1: ...`, `// Phase X`). All code comments must instruct the human reader on domain logic, invariants, and algorithms.

---

## 2. Development Setup

### Prerequisites

- **macOS** on Apple Silicon (M1/M2/M3/M4 recommended) with APFS filesystem.
- **Rust Toolchain**: Rust 1.80+ (verified up to latest stable `1.99.1` via `rustup update`).
- **LLVM Tools & Coverage**:
  ```bash
  rustup component add llvm-tools
  brew install cargo-llvm-cov
  ```
- **Supply-Chain Security & License Auditing**:
  ```bash
  brew install cargo-deny
  # or: cargo install --locked cargo-deny
  ```
- **Node.js & Playwright** (for visual catalog tests):
  ```bash
  cd tests/e2e && npm install
  npx playwright install chromium
  ```
- **Python 3.10+, Pytest & `uv`** (for gazetteer and inference checks):
  ```bash
  brew install uv
  pip install pytest
  ```

### Git Hooks Installation

Install the pre-push verification hook to automatically validate all checks prior to pushing:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-push
```

---

## 3. Engineering Workflow & TDD

We follow strict **Test-Driven Development (TDD)**:

1. **Write a Failing Test First (RED)**:
   Add unit or integration tests reproducing the bug or defining the new capability.
   Integration tests should be added to `tests/test_suite/test_<feature>.rs` and registered in `tests/test_suite/main.rs`.
   Run `cargo test --test test_suite -- <test_name>` and verify failure.

2. **Implement Minimal Code (GREEN)**:
   Write only enough code to pass the test.
3. **Refactor & Document (REFACTOR)**:
   Format code, satisfy strict linter rules, audit dependencies, write comprehensive Rustdoc comments, and verify:
   ```bash
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   cargo deny check
   RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
   ```

   **Strict Linter & Safety Standards (`Cargo.toml`)**:
   - `unsafe_code = "deny"`: Zero `unsafe` blocks allowed.
   - `pedantic = { level = "deny", priority = -1 }`: Strict idiomatic Rust enforced.
   - `unwrap_used = "deny"`, `panic = "deny"`, `todo = "deny"`: No unhandled unwrap or panic calls in production code. Use idiomatic error handling via `Result` and `thiserror`.
   - `redundant_clone = "deny"`: Eliminates unnecessary allocations and clones.

---

## 4. Verification & Testing Tiers

### Fast Verification Pipeline (`scripts/verify.sh`)
For rapid local checks during development, run the canonical 6-stage verification script:
```bash
./scripts/verify.sh
```
This executes:
1. `cargo fmt --check`
2. `cargo check --all-targets`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo test --lib --test test_suite --test test_mock_directory_structure_e2e`
5. `cargo deny check`
6. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`

### Full Pre-Push Verification Suite (`.githooks/pre-push`)
Before pushing code or submitting a Pull Request, all verification tiers must pass:

| Step | Verification Gate | Command |
|---|---|---|
| **Preflight** | Dependency Supply Chain & Licenses | `cargo deny check` |
| **1** | Formatting | `cargo fmt --check` |
| **2** | Clippy Linter | `cargo clippy --all-targets --all-features -- -D warnings` |
| **3** | Unit & Integration Tests (Coverage) | `cargo llvm-cov --fail-under-lines 80 --lib --test test_suite --test test_mock_directory_structure_e2e` |
| **4** | Python Inference Tests | `uv run --with "torch,open-clip-torch,safetensors,ultralytics,pillow" python3 -m unittest discover -s tests_python` |
| **5** | Mock Directory Structure E2E | `cargo test --test test_mock_directory_structure_e2e` |
| **6** | Release Binary E2E (if present) | Tested automatically if binary exists in `dist/` or `target/release/` |
| **7** | Python Gazetteer Integrity | `pytest tests/test_build_gazetteer.py` |
| **8** | Rust Documentation Verification | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` |

You can run the full suite in one command:
```bash
./.githooks/pre-push
```

---

## 5. Documentation Guidelines

- **Module Headers (`//!`)**: Every source file in `src/` must begin with a `//!` header explaining module purpose, invariants, and algorithms.
- **Public Items (`///`)**: Every `pub` function, struct, enum, and trait must be documented with `# Arguments`, `# Returns`, `# Errors`, and `# Invariants` where applicable.
- **Mathematical Formats**: Render equations using LaTeX syntax (`$...$` inline, `$$...$$` block).
- **Intra-Doc Links**: Use markdown links with backticks (e.g. `[`AssetBundle`]`, `[`ConfidenceMode`]`).
- **HTML Tags**: Escape any angle-bracketed templates or HTML tags in code blocks or backticks to prevent rustdoc syntax warnings.

---

## 6. Pull Request Checklist

When submitting a pull request:

- [ ] Branch created from latest `main` (e.g. `feat/my-feature` or `fix/my-fix`).
- [ ] Tests added for new functionality or bug fixes.
- [ ] Code formatted with `cargo fmt`.
- [ ] No clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- [ ] Dependency audit passes (`cargo deny check`).
- [ ] Rustdoc passes without warnings (`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`).
- [ ] Local `./.githooks/pre-push` passes all 8 checks.
- [ ] Code coverage remains $\ge 80\%$ (target $>95\%$).
- [ ] Clear PR title and description outlining the changes and test evidence.
