#!/usr/bin/env bash
# panic_debt_classification.sh
#
# Issue #10869 Phase 0: classify every static unwrap_call/expect_call/
# panic_macro (plus todo_macro/unimplemented_macro) site under the panic-free
# ratchet's production roots into one of four buckets — test-only,
# build-time invariant, cache-corruption boundary, user-input reachable.
#
# This is a REPORT GENERATOR, not a CI gate: it always exits 0 (parse errors
# aside) and is never wired into check_panic_free_ratchet.sh,
# premerge_gate.sh, or any other build-failing gate. Re-run it any time to
# regenerate the committed docs/vm/PANIC_DEBT_CLASSIFICATION.tsv snapshot;
# there is no ratchet on its output.
#
# Usage:
#   bash scripts/panic_debt_classification.sh
#   bash scripts/panic_debt_classification.sh --out docs/vm/PANIC_DEBT_CLASSIFICATION.tsv

set -euo pipefail

cd "$(dirname "$0")/.."

if [[ ! -f Cargo.toml || ! -d subset_julia_vm_ffi/src ]]; then
    echo "ERROR: run from the repository root." >&2
    exit 2
fi

exec python3 scripts/panic_debt_classification.py "$@"
