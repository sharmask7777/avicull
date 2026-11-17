#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "\n${BOLD}${BLUE}==============================================${NC}"
echo -e "${BOLD}${BLUE}  Targeted Mutation Testing (cargo-mutants)   ${NC}"
echo -e "${BOLD}${BLUE}==============================================${NC}\n"

# Check if cargo-mutants is installed
if ! command -v cargo-mutants >/dev/null 2>&1; then
    echo -e "${YELLOW}⚠ 'cargo-mutants' is not installed or not in PATH.${NC}"
    echo -e "${YELLOW}  To install cargo-mutants, run:${NC}"
    echo -e "${YELLOW}    cargo install --locked cargo-mutants${NC}\n"
    exit 0
fi

# Honor Apple Silicon core preservation
JOBS="${CARGO_BUILD_JOBS:-4}"

echo -e "${BOLD}Running targeted mutations on src/types.rs and src/burst.rs...${NC}"
cargo mutants \
    --file src/types.rs \
    --file src/burst.rs \
    --jobs "$JOBS" \
    --test-tool cargo \
    -- --lib --test test_suite

echo -e "\n${GREEN}✓ Mutation testing completed successfully.${NC}\n"
