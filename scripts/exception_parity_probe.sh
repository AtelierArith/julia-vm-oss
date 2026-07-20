#!/usr/bin/env bash
# exception_parity_probe.sh
#
# Issue #10813 Phase 0: run a fixed corpus of error-triggering Julia
# constructs under BOTH upstream `julia` and `sjulia`, in bare and
# try/catch-wrapped form, and report per-construct exception TYPE parity and
# catchability (raise-LAYER) parity as a TSV snapshot.
#
# This is a REPORT GENERATOR, not a CI gate: it always exits 0 (interpreter
# invocation failures aside) and is never wired into a build-failing gate.
# Re-run it any time to regenerate docs/vm/EXCEPTION_PARITY_PROBE.tsv; there
# is no ratchet on its output (Phase 1 decomposes the ratchet work).
#
# Usage:
#   bash scripts/exception_parity_probe.sh
#   bash scripts/exception_parity_probe.sh --sjulia target/release/sjulia --julia julia
#   bash scripts/exception_parity_probe.sh --out docs/vm/EXCEPTION_PARITY_PROBE.tsv

set -euo pipefail

cd "$(dirname "$0")/.."

if [[ ! -f Cargo.toml || ! -d subset_julia_vm_ffi/src ]]; then
    echo "ERROR: run from the repository root." >&2
    exit 2
fi

SJULIA_BIN="target/release/sjulia"
if [[ ! -x "$SJULIA_BIN" ]]; then
    SJULIA_BIN="target/dev-fast/sjulia"
fi

ARGS=()
if [[ -x "$SJULIA_BIN" ]]; then
    ARGS+=(--sjulia "$SJULIA_BIN")
elif [[ "$*" != *--sjulia* ]]; then
    echo "ERROR: no sjulia binary found at target/release/sjulia or target/dev-fast/sjulia," \
        "and --sjulia was not passed explicitly." >&2
    echo "Build one first, e.g.:" >&2
    echo "  cargo build --profile dev-fast -p subset_julia_vm --bin sjulia --features repl" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "WARNING: 'julia' not found on PATH; the probe needs upstream julia to compare against." >&2
fi

exec python3 scripts/exception_parity_probe.py "${ARGS[@]}" "$@"
