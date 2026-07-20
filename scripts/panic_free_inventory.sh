#!/usr/bin/env bash
# panic_free_inventory.sh
#
# Measure current panic-prone lint warnings and native FFI catch_unwind coverage.
# This is a developer inventory for Issue #8705, not a CI ratchet.
#
# Usage:
#   bash scripts/panic_free_inventory.sh --run-clippy
#   bash scripts/panic_free_inventory.sh --skip-clippy
#   bash scripts/panic_free_inventory.sh --clippy-jsonl target/panic-free-inventory/clippy.jsonl

set -euo pipefail

if [[ ! -f Cargo.toml || ! -d subset_julia_vm_ffi/src ]]; then
    echo "ERROR: run from the repository root." >&2
    exit 2
fi

exec python3 scripts/panic_free_inventory.py "$@"
