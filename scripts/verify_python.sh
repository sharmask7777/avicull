#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "\n${BOLD}${BLUE}==============================================${NC}"
echo -e "${BOLD}${BLUE}  avicull Python Verification Pipeline        ${NC}"
echo -e "${BOLD}${BLUE}==============================================${NC}\n"

# Preflight check: Ensure uv or python3 is available
if ! command -v uv >/dev/null 2>&1; then
    echo -e "${RED}✗ 'uv' is not installed or not in PATH.${NC}"
    echo -e "${RED}  Install uv via: brew install uv (or: curl -LsSf https://astral.sh/uv/install.sh | sh)${NC}\n"
    exit 1
fi

# Detect whether network/PyPI is accessible; use --offline if offline or sandbox blocks outbound network
UV_EXTRA_FLAG=""
if ! curl -s --connect-timeout 2 -I https://pypi.org >/dev/null 2>&1; then
    UV_EXTRA_FLAG="--offline"
fi

# 1. Linting with Ruff
echo -e "${BOLD}[1/5] Checking Python code formatting and linting (ruff check)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with ruff ruff check .; then
    echo -e "${GREEN}✓ ruff check passed.${NC}\n"
else
    echo -e "${RED}✗ ruff check reported lint violations.${NC}\n"
    exit 1
fi

# 2. Format check with Ruff
echo -e "${BOLD}[2/5] Checking Python code formatting style (ruff format --check)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with ruff ruff format --check .; then
    echo -e "${GREEN}✓ ruff format check passed.${NC}\n"
else
    echo -e "${RED}✗ ruff format reported formatting differences. Run 'uv run --with ruff ruff format .' to fix.${NC}\n"
    exit 1
fi

# 3. Type checking with Mypy
echo -e "${BOLD}[3/5] Running static type checking (mypy scripts/)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with mypy --with pytest mypy scripts/; then
    echo -e "${GREEN}✓ mypy type checking passed.${NC}\n"
else
    echo -e "${RED}✗ mypy reported type check errors.${NC}\n"
    exit 1
fi

# 4. Architectural linting with import-linter
echo -e "${BOLD}[4/5] Checking architecture contracts (lint-imports)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with import-linter lint-imports; then
    echo -e "${GREEN}✓ Architectural import contracts satisfied.${NC}\n"
else
    echo -e "${RED}✗ Architectural import contracts violated.${NC}\n"
    exit 1
fi

# 5. Unit & Integration tests with Coverage Gate
echo -e "${BOLD}[5/5] Running Python test suite with blocking coverage gate (pytest --cov-fail-under=80)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with pytest --with pytest-cov --with torch --with safetensors --with open-clip-torch --with ultralytics --with pillow \
    pytest tests/ tests_python/ --cov=scripts --cov-fail-under=80 --cov-report=term-missing; then
    echo -e "${GREEN}✓ All Python tests passed and met blocking code coverage gate (>= 80%).${NC}\n"
else
    echo -e "${RED}✗ Python tests failed or code coverage fell below minimum threshold (80%).${NC}\n"
    exit 1
fi

echo -e "${BOLD}${GREEN}==============================================${NC}"
echo -e "${BOLD}${GREEN}  Python Verification Pipeline Succeeded!     ${NC}"
echo -e "${BOLD}${GREEN}==============================================${NC}\n"
exit 0
