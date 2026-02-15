#!/usr/bin/env bash
# Run cargo test with the correct Python environment.
# uv's standalone Python needs explicit paths for the linker & runtime.
set -euo pipefail

UV_PYTHON_DIR="$HOME/.local/share/uv/python/cpython-3.13.0-macos-aarch64-none"

export LIBRARY_PATH="${UV_PYTHON_DIR}/lib:${LIBRARY_PATH:-}"
export DYLD_LIBRARY_PATH="${UV_PYTHON_DIR}/lib:${DYLD_LIBRARY_PATH:-}"

source "$(dirname "$0")/.venv/bin/activate"

# Must be set AFTER activate, which unsets PYTHONHOME
export PYTHONHOME="${UV_PYTHON_DIR}"

cargo test "$@"
