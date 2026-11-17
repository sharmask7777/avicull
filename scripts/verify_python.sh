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

# Limit BLAS/PyTorch thread proliferation to prevent memory exhaustion and UI latency
export OMP_NUM_THREADS="${OMP_NUM_THREADS:-2}"
export MKL_NUM_THREADS="${MKL_NUM_THREADS:-2}"
export OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-2}"
export VECLIB_MAXIMUM_THREADS="${VECLIB_MAXIMUM_THREADS:-2}"
export NUMEXPR_NUM_THREADS="${NUMEXPR_NUM_THREADS:-2}"

# 1. Linting with Ruff
echo -e "${BOLD}[1/5] Checking Python code formatting and linting (ruff check)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with ruff ruff check .; then
    echo -e "${GREEN}✓ ruff check passed.${NC}\n"
else
    echo -e "${RED}✗ ruff check reported lint violations.${NC}"
    echo -e "${YELLOW}  [Sensor Guidance for Coding Agents]:${NC}"
    echo -e "${YELLOW}  • Complexity violations (C901, PLR0912, PLR0913, PLR0915): Refactor monolithic functions${NC}"
    echo -e "${YELLOW}    into modular, private helper methods. Do NOT add blanket '# noqa' or raise thresholds.${NC}"
    echo -e "${YELLOW}  • Error handling (TRY rules): Use proper exception types and keep try blocks minimal.${NC}\n"
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
    echo -e "${RED}✗ mypy reported type check errors.${NC}"
    echo -e "${YELLOW}  [Sensor Guidance for Coding Agents]:${NC}"
    echo -e "${YELLOW}  • Add explicit type annotations to all parameters and returns.${NC}"
    echo -e "${YELLOW}  • Use 'X | None' instead of Optional[X], and concrete collections (e.g. list[T]).${NC}\n"
    exit 1
fi

# 4. Architectural linting with import-linter
echo -e "${BOLD}[4/5] Checking architecture contracts (lint-imports)...${NC}"
if uv run ${UV_EXTRA_FLAG:+"$UV_EXTRA_FLAG"} --with import-linter lint-imports; then
    echo -e "${GREEN}✓ Architectural import contracts satisfied.${NC}\n"
else
    echo -e "${RED}✗ Architectural import contracts violated.${NC}"
    echo -e "${YELLOW}  [Sensor Guidance for Coding Agents]:${NC}"
    echo -e "${YELLOW}  • Offline data prep pipelines (build_gazetteer, build_taxonomy_embeddings, export_models)${NC}"
    echo -e "${YELLOW}    must NOT import from runtime inference (scripts.infer). Keep pipelines decoupled!${NC}\n"
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
