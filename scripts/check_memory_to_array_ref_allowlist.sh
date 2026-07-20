#!/usr/bin/env bash
# Issue #4008 — keep Memory -> Array compatibility conversions retired.
#
# `memory_to_array_ref` used to be a migration bridge while Array semantics
# moved toward Memory primitives plus Pure Julia Array wrappers. It is now
# retired: any reintroduction must be issue-driven and justified against the
# upstream Memory / Array boundary in `julia/base/genericmemory.jl` and
# `julia/base/array.jl`.

set -euo pipefail

MATCHES_FILE="$(mktemp)"
trap 'rm -f "$MATCHES_FILE"' EXIT

rg -n 'memory_to_array_ref\(' \
    subset_julia_vm/src \
    subset_julia_vm_lowering/src \
    subset_julia_vm_compile/src \
    subset_julia_vm_vm/src \
    subset_julia_vm_bytecode/src \
    subset_julia_vm_types/src \
    subset_julia_vm_ir/src \
    subset_julia_vm_ffi/src \
    subset_julia_vm_parser/src \
    subset_julia_vm_web/src \
    > "$MATCHES_FILE" || true

if [[ -s "$MATCHES_FILE" ]]; then
    cat "$MATCHES_FILE"
    echo ""
    echo "FAILED: memory_to_array_ref compatibility bridge was reintroduced (Issue #4008)."
    echo "Prefer Memory primitives or Pure Julia Array wrapper dispatch."
    exit 1
fi

echo "OK: memory_to_array_ref compatibility bridge is absent (Issue #4008)."
