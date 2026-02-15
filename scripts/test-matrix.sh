#!/usr/bin/env bash
# Run the test suite across multiple Python versions using uv.
#
# Usage:
#   ./scripts/test-matrix.sh            # all versions, GIL + no-GIL
#   ./scripts/test-matrix.sh 3.13       # single version
#   ./scripts/test-matrix.sh 3.13t      # single free-threaded version
#   ./scripts/test-matrix.sh bench      # benchmark suite (current venv)
#
# Prerequisites:
#   - uv >= 0.4  (brew install uv)
#   - Rust toolchain (for maturin)
#   - Redis server running for integration / benchmark tests
#
# Each version gets its own ephemeral venv via `uv run --python <ver>`.
# maturin builds the extension into that venv before pytest runs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

# Colors (disabled when not a terminal)
if [[ -t 1 ]]; then
  GREEN=$'\033[0;32m'
  RED=$'\033[0;31m'
  YELLOW=$'\033[0;33m'
  CYAN=$'\033[0;36m'
  RESET=$'\033[0m'
else
  GREEN="" RED="" YELLOW="" CYAN="" RESET=""
fi

# GIL-enabled versions
GIL_VERSIONS=("3.11" "3.12" "3.13" "3.14")
# Free-threaded (no-GIL) versions — CPython 3.13t+
NOGIL_VERSIONS=("3.13t" "3.14t")

PASSED=()
FAILED=()
SKIPPED=()

run_tests() {
  local ver="$1"
  shift
  local extra_args=("$@")

  echo ""
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"
  echo "${CYAN}  Python ${ver}${RESET}"
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"

  # Check if uv can find this Python version
  if ! uv python find "$ver" &>/dev/null; then
    echo "${YELLOW}  ⚠ Python ${ver} not installed — installing via uv${RESET}"
    if ! uv python install "$ver" 2>/dev/null; then
      echo "${YELLOW}  ⚠ Python ${ver} not available — skipping${RESET}"
      SKIPPED+=("$ver")
      return 0
    fi
  fi

  echo "  Building extension..."
  if ! uv run --python "$ver" --isolated --extra dev --with maturin \
       maturin develop --release 2>&1 | tail -3; then
    echo "${RED}  ✗ Build failed for Python ${ver}${RESET}"
    FAILED+=("$ver")
    return 1
  fi

  echo "  Running tests..."
  if uv run --python "$ver" --isolated --extra dev --with maturin \
     pytest tests/python/ -v --tb=short "${extra_args[@]}"; then
    echo "${GREEN}  ✓ Python ${ver} — passed${RESET}"
    PASSED+=("$ver")
  else
    echo "${RED}  ✗ Python ${ver} — failed${RESET}"
    FAILED+=("$ver")
  fi
}

run_bench() {
  echo ""
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"
  echo "${CYAN}  Benchmark: pyrsedis vs falkordb-py${RESET}"
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"

  echo "  Building extension (release)..."
  uv run --extra dev --with maturin \
     maturin develop --release 2>&1 | tail -3

  echo "  Running benchmarks..."
  uv run --extra dev --with maturin \
     pytest tests/python/test_benchmark.py -v -s --tb=short
}

print_summary() {
  echo ""
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"
  echo "${CYAN}  Summary${RESET}"
  echo "${CYAN}───────────────────────────────────────────────${RESET}"
  [[ ${#PASSED[@]} -gt 0 ]]  && echo "  ${GREEN}Passed:${RESET}  ${PASSED[*]}"
  [[ ${#FAILED[@]} -gt 0 ]]  && echo "  ${RED}Failed:${RESET}  ${FAILED[*]}"
  [[ ${#SKIPPED[@]} -gt 0 ]] && echo "  ${YELLOW}Skipped:${RESET} ${SKIPPED[*]}"
  echo "${CYAN}═══════════════════════════════════════════════${RESET}"

  [[ ${#FAILED[@]} -eq 0 ]]
}

# ── Main ────────────────────────────────────────────────────────────

if [[ $# -eq 0 ]]; then
  # Run all versions
  for ver in "${GIL_VERSIONS[@]}"; do
    run_tests "$ver"
  done
  for ver in "${NOGIL_VERSIONS[@]}"; do
    run_tests "$ver"
  done
  print_summary
elif [[ "$1" == "bench" ]]; then
  run_bench
else
  # Run specific version(s)
  for ver in "$@"; do
    run_tests "$ver"
  done
  print_summary
fi
