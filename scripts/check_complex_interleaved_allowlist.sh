#!/usr/bin/env bash
# check_complex_interleaved_allowlist.sh
#
# Containment audit for the interleaved-Complex array Rust specialization
# (Issue #7876 / docs/COMPARISION.md P2; RUST_BOUNDARY_JUSTIFICATION.md
# condition 4 — the no-JIT performance boundary).
#
# sjulia stores Complex arrays in an interleaved `[re0, im0, re1, im1, ...]` Rust
# representation with dedicated fast paths (broadcast / matmul / index / mutation /
# reduction). That is a deliberate performance tradeoff, kept ONLY because the VM
# is no-JIT. Condition 4 is an easy-to-overextend justification ("for speed"), so
# this audit CONTAINS the specialization: the set of files that reference the
# interleaved representation is pinned to an explicit allowlist. A NEW file that
# introduces interleaved-Complex code fails the audit and must be justified
# (condition 4 + Issue number comment) and added to the allowlist below.
#
# Direction that matters: NEW site outside the allowlist = ERROR (proliferation).
# A stale allowlist entry (no longer matches) = non-fatal NOTE (remove it when
# convenient); it does not weaken containment.
#
# Marker: the case-insensitive substring "interleav" under subset_julia_vm/src/vm.
# This is intentionally broad (matches comments too): a new file that even
# *describes* interleaved Complex storage is a new specialization surface.
#
# Usage (from repository root):
#   ./scripts/check_complex_interleaved_allowlist.sh
#
# Exit code: 0 = contained, 1 = a non-allowlisted file introduced interleaved code
#            (or run from the wrong directory).
#
# bash 3.2 compatible (macOS stock): no associative arrays, no mapfile/readarray.

set -euo pipefail

VM_DIR="subset_julia_vm/src/vm"

if [[ ! -d "$VM_DIR" ]]; then
    echo "ERROR: $VM_DIR not found. Run from the repository root." >&2
    exit 1
fi

# ---- Allowlist: files permitted to carry interleaved-Complex specialization ---
# Measured 2026-06-26 on main @ Issue #7876 (18 files). Keep sorted for review.
ALLOWLIST="
subset_julia_vm/src/vm/broadcast.rs
subset_julia_vm/src/vm/builtins_arrays.rs
subset_julia_vm/src/vm/builtins_linalg.rs
subset_julia_vm/src/vm/dynamic_ops/helpers.rs
subset_julia_vm/src/vm/exec/array_basic.rs
subset_julia_vm/src/vm/exec/array_index.rs
subset_julia_vm/src/vm/exec/array_mutate.rs
subset_julia_vm/src/vm/exec/struct_ops.rs
subset_julia_vm/src/vm/matmul/multiply.rs
subset_julia_vm/src/vm/matmul/scalar.rs
subset_julia_vm/src/vm/value/array_data.rs
subset_julia_vm/src/vm/value/array_element.rs
subset_julia_vm/src/vm/value/array_value/access.rs
subset_julia_vm/src/vm/value/array_value/mod.rs
subset_julia_vm/src/vm/value/array_value/mutation.rs
subset_julia_vm/src/vm/value/array_wrapper.rs
subset_julia_vm/src/vm/value/memory_value.rs
subset_julia_vm/src/vm/value/struct_instance.rs
"

is_allowlisted() {
    # $1 = path. Returns 0 if present in ALLOWLIST.
    local needle="$1" entry
    while IFS= read -r entry; do
        [[ -z "$entry" ]] && continue
        [[ "$entry" == "$needle" ]] && return 0
    done <<EOF
$ALLOWLIST
EOF
    return 1
}

# ---- Find current interleaved-Complex sites ---------------------------------
current_files=$(grep -rilE 'interleav' "$VM_DIR" --include='*.rs' 2>/dev/null | sort -u || true)

# ---- 1. New (non-allowlisted) sites = ERROR ---------------------------------
violations=0
while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    if ! is_allowlisted "$f"; then
        if [[ "$violations" -eq 0 ]]; then
            echo "ERROR: interleaved-Complex specialization appeared in non-allowlisted file(s):"
        fi
        echo "  $f"
        violations=$((violations + 1))
    fi
done <<EOF
$current_files
EOF

# ---- 2. Stale allowlist entries = non-fatal NOTE ----------------------------
stale=0
while IFS= read -r entry; do
    [[ -z "$entry" ]] && continue
    if [[ ! -f "$entry" ]]; then
        echo "NOTE: allowlist entry no longer exists (remove it): $entry"
        stale=$((stale + 1))
    elif ! grep -qilE 'interleav' "$entry" 2>/dev/null; then
        echo "NOTE: allowlist entry no longer references interleaved storage (remove it): $entry"
        stale=$((stale + 1))
    fi
done <<EOF
$ALLOWLIST
EOF

# ---- Verdict ----------------------------------------------------------------
if [[ "$violations" -gt 0 ]]; then
    echo ""
    echo "FAILED: $violations file(s) introduced interleaved-Complex specialization"
    echo "outside the allowlist (Issue #7876)."
    echo ""
    echo "The interleaved-Complex Rust fast path is a no-JIT performance tradeoff"
    echo "(RUST_BOUNDARY_JUSTIFICATION.md condition 4). New specialization sites must:"
    echo "  1. be justified with a '// Boundary: condition 4 (no-JIT perf), Issue #NNNN'"
    echo "     comment, AND"
    echo "  2. guarantee a pure-Julia fallback (see"
    echo "     tests/fixtures/complex/complex_array_fallback_parity_7876.jl), AND"
    echo "  3. be added to the ALLOWLIST in this script in the same PR."
    echo "Otherwise, route Complex array work through the pure-Julia scalar path."
    exit 1
fi

if [[ "$stale" -gt 0 ]]; then
    echo ""
    echo "OK (with $stale stale allowlist entr(y/ies) to prune — non-fatal)."
    exit 0
fi

echo "OK: interleaved-Complex specialization confined to the allowlist (18 files)."
exit 0
