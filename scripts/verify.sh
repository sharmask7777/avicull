#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "\n${BOLD}${BLUE}==============================================${NC}"
echo -e "${BOLD}${BLUE}  avicull Verification Pipeline               ${NC}"
echo -e "${BOLD}${BLUE}==============================================${NC}\n"

# Dynamically set CARGO_BUILD_JOBS and TEST_THREADS to physical performance cores if on Apple Silicon,
# capped to prevent memory pressure on 16GB machines while preserving UI responsiveness.
if [ -z "${CARGO_BUILD_JOBS:-}" ]; then
    PERF_CORES=$(sysctl -n hw.perflevel0.logicalcpu 2>/dev/null || true)
    if [ -n "$PERF_CORES" ] && [ "$PERF_CORES" -gt 0 ]; then
        if [ "$PERF_CORES" -gt 4 ]; then
            export CARGO_BUILD_JOBS=4
        else
            export CARGO_BUILD_JOBS="$PERF_CORES"
        fi
    fi
fi

# Limit test threads to 4 to prevent RAM saturation during concurrent integration tests
RUST_TEST_THREADS="${RUST_TEST_THREADS:-4}"

# Preflight Tooling Checks
command -v cargo >/dev/null 2>&1 || {
    echo -e "${RED}✗ 'cargo' is not installed or not in PATH.${NC}\n"
    exit 1
}

# 1. Rust Code Formatting
echo -e "${BOLD}[1/8] Checking Rust code formatting (cargo fmt --check)...${NC}"
if cargo fmt --check; then
    echo -e "${GREEN}✓ cargo fmt passed.${NC}\n"
else
    echo -e "${RED}✗ cargo fmt failed. Run 'cargo fmt' to fix formatting.${NC}\n"
    exit 1
fi

# 2. Rust Compiler Check
echo -e "${BOLD}[2/8] Running compiler check on all targets (cargo check --all-targets)...${NC}"
if cargo check --all-targets; then
    echo -e "${GREEN}✓ cargo check passed.${NC}\n"
else
    echo -e "${RED}✗ cargo check failed.${NC}\n"
    exit 1
fi

# 3. Rust Linter (Clippy)
echo -e "${BOLD}[3/8] Running strict Rust linter (cargo clippy --all-targets --all-features -- -D warnings)...${NC}"
if cargo clippy --all-targets --all-features -- -D warnings; then
    echo -e "${GREEN}✓ cargo clippy passed.${NC}\n"
else
    echo -e "${RED}✗ cargo clippy reported warnings or errors.${NC}"
    echo -e "${YELLOW}  [Sensor Guidance for Coding Agents]:${NC}"
    echo -e "${YELLOW}  • cognitive_complexity error: Refactor large functions into smaller modular helpers.${NC}"
    echo -e "${YELLOW}  • unwrap_used: Replace .unwrap() with idiomatic error propagation (?) or match.${NC}"
    echo -e "${YELLOW}  • panic! / todo!: Production code must never panic or leave unhandled stubs.${NC}\n"
    exit 1
fi

# 4. Unit & Integration Tests
echo -e "${BOLD}[4/8] Running unit and integration tests (cargo test --lib --test test_suite --test test_mock_directory_structure_e2e -- --test-threads=${RUST_TEST_THREADS})...${NC}"
if cargo test --lib --test test_suite --test test_mock_directory_structure_e2e -- --test-threads="${RUST_TEST_THREADS}"; then
    echo -e "${GREEN}✓ cargo test passed.${NC}\n"
else
    echo -e "${RED}✗ cargo test failed.${NC}\n"
    exit 1
fi

# 5. Python Verification Suite (ruff, mypy, pytest with >= 80% coverage)
echo -e "${BOLD}[5/8] Running Python verification suite (scripts/verify_python.sh)...${NC}"
if [ -x "./scripts/verify_python.sh" ]; then
    if ./scripts/verify_python.sh; then
        echo -e "${GREEN}✓ Python verification suite passed.${NC}\n"
    else
        echo -e "${RED}✗ Python verification suite failed.${NC}\n"
        exit 1
    fi
else
    echo -e "${RED}✗ scripts/verify_python.sh not found or not executable.${NC}\n"
    exit 1
fi

# 6. Cargo Deny Check (Supply-Chain Security, Bans & Licenses)
echo -e "${BOLD}[6/8] Checking dependency supply-chain security and licenses (cargo deny check)...${NC}"
if command -v cargo-deny >/dev/null 2>&1 || cargo deny --version >/dev/null 2>&1; then
    if cargo deny check; then
        echo -e "${GREEN}✓ cargo deny check passed.${NC}\n"
    else
        echo -e "${RED}✗ cargo deny check reported security advisory, license, or ban violations.${NC}\n"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ 'cargo-deny' is not installed.${NC}"
    echo -e "${YELLOW}  Install via: brew install cargo-deny (or: cargo install --locked cargo-deny)${NC}\n"
fi

# 7. Playwright E2E UI Smoke Test
echo -e "${BOLD}[7/8] Running Playwright E2E UI smoke test...${NC}"
if command -v npm >/dev/null 2>&1 && [ -d "tests/e2e/node_modules" ]; then
    if npm --prefix tests/e2e test -- bird_catalog.spec.ts --project=chromium; then
        echo -e "${GREEN}✓ Playwright E2E UI smoke test passed.${NC}\n"
    else
        echo -e "${RED}✗ Playwright E2E UI smoke test failed.${NC}\n"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ Playwright e2e tests skipped (tests/e2e/node_modules missing or npm not found).${NC}\n"
fi

# 8. Rust Documentation Verification
echo -e "${BOLD}[8/8] Checking Rust documentation (RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps)...${NC}"
if RUSTDOCFLAGS="-D warnings" cargo doc --no-deps; then
    echo -e "${GREEN}✓ Rust documentation check passed (zero warnings).${NC}\n"
else
    echo -e "${RED}✗ Rust documentation check reported warnings or errors.${NC}\n"
    exit 1
fi

echo -e "${BOLD}${GREEN}==============================================${NC}"
echo -e "${BOLD}${GREEN}  Verification Pipeline Succeeded!            ${NC}"
echo -e "${BOLD}${GREEN}==============================================${NC}\n"
exit 0
