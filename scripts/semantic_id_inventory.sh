#!/usr/bin/env bash
# semantic_id_inventory.sh
#
# Issue #10459 Phase 0: mechanically inventory bare-name identity sites
# (HashMap<String, ...>-shaped tables, `*_by_name` lookups, and the six
# scripts/check_name_based_lookup.sh anchors) across production source,
# classified by identity domain / layer / migration difficulty.
#
# This is a REPORT GENERATOR, not a CI gate: it always exits 0 (parse errors
# aside) and is never wired into check_name_based_lookup.sh,
# premerge_gate.sh, or any other build-failing gate. Re-run it any time to
# regenerate the committed docs/vm/SEMANTIC_ID_INVENTORY.tsv snapshot; there
# is no ratchet on its output.
#
# Usage:
#   bash scripts/semantic_id_inventory.sh
#   bash scripts/semantic_id_inventory.sh --out docs/vm/SEMANTIC_ID_INVENTORY.tsv

set -euo pipefail

cd "$(dirname "$0")/.."

if [[ ! -f Cargo.toml || ! -d subset_julia_vm_types/src ]]; then
    echo "ERROR: run from the repository root." >&2
    exit 2
fi

exec python3 scripts/semantic_id_inventory.py "$@"
