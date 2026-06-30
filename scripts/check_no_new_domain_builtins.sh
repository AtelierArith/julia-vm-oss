#!/usr/bin/env bash
# check_no_new_domain_builtins.sh
#
# POSITIVE boundary audit (Issue #7878 / docs/COMPARISION.md P4).
#
# The existing scripts/check_*.sh audits are NEGATIVE: they keep already-retired
# carriers (Value::Array / Value::Dict / Value::Set, memory_to_array_ref, …)
# from coming back. None of them stop the Layer-2 Rust builtin surface from
# GROWING with new domain logic that has no performance justification.
#
# This audit is the missing POSITIVE gate: it ratchets two metrics for the
# Layer-2 Rust builtin surface (`builtins.rs` + `vm/builtins_*.rs`) and FAILS
# when either grows past its baseline:
#
#   1. BuiltinId enum variant count   (hard ratchet — exact)
#   2. Layer-2 total LOC              (soft ratchet — tolerance band)
#
# Rationale (CLAUDE.md principles #2/#3 "Pure Julia First"): the no-JIT VM keeps
# performance-critical fast paths in Rust on purpose (RUST_BOUNDARY_JUSTIFICATION.md
# conditions 1–4: OS/HW, external C/Fortran lib, VM-internal metadata, no-JIT
# perf boundary). Anything that does NOT meet one of those conditions belongs in
# Pure Julia under subset_julia_vm/src/julia/. Without a positive gate, Layer-2
# bloat creeps in unnoticed (docs/COMPARISION.md found ~700–1,000 lines of such
# domain logic, tracked by #7875).
#
# WHEN THIS AUDIT FIRES because you intentionally added a builtin:
#   - Confirm the new handler meets RUST_BOUNDARY_JUSTIFICATION.md condition 1–4.
#   - Add a justification comment next to the new `BuiltinId::` variant of the
#     form:  `// Boundary: condition N (<why>), Issue #NNNN`
#   - Bump the matching baseline constant below IN THE SAME PR, citing the Issue.
#   - If you cannot justify a condition, implement it in Pure Julia instead.
#
# WHEN THE COUNT/LOC DROPS (a Pure Julia migration removed builtins — good!):
#   - Lower the baseline constant(s) below so the ratchet tightens. This is a
#     reminder, not an error (the gate stays correct, just looser than it could
#     be).
#
# Usage (from repository root):
#   ./scripts/check_no_new_domain_builtins.sh
#
# Exit code: 0 = within baseline, 1 = grew past baseline (or run from wrong dir).
#
# bash 3.2 compatible (macOS stock): no associative arrays, no mapfile/readarray.

set -euo pipefail

# ---- Baselines (update deliberately; see header) ----------------------------
# Measured 2026-06-30 on main during milestone-56 structural debt ratchet
# registration (Issues #7878/#8327-#8337).
BASELINE_BUILTIN_COUNT=246
BASELINE_BUILTIN_LOC=11011
# LOC tolerance absorbs comment/format churn while still catching real growth
# (a large new match arm in an existing handler that adds no new BuiltinId).
LOC_TOLERANCE=300
# -----------------------------------------------------------------------------

BUILTINS_RS="subset_julia_vm/src/builtins.rs"
VM_DIR="subset_julia_vm/src/vm"

if [[ ! -f "$BUILTINS_RS" ]]; then
    echo "ERROR: $BUILTINS_RS not found. Run from the repository root." >&2
    exit 1
fi

# ---- Metric 1: BuiltinId enum variant count ---------------------------------
# Extract identifiers declared inside `pub enum BuiltinId { ... }`, skipping
# comment lines, and count distinct variants.
current_count=$(
    awk '/pub enum BuiltinId \{/{f=1;next} f&&/^\}/{f=0} f' "$BUILTINS_RS" \
        | grep -vE '^[[:space:]]*//' \
        | grep -oE '^[[:space:]]*[A-Z][A-Za-z0-9_]*' \
        | sed 's/^[[:space:]]*//' \
        | sort -u \
        | grep -c . \
        || true
)
current_count=${current_count//[^0-9]/}
: "${current_count:=0}"

# ---- Metric 2: Layer-2 total LOC --------------------------------------------
loc_total=0
for f in "$BUILTINS_RS" "$VM_DIR"/builtins_*.rs; do
    [[ -f "$f" ]] || continue
    n=$(wc -l < "$f" | tr -d ' ')
    loc_total=$((loc_total + n))
done

# ---- Evaluate ---------------------------------------------------------------
failed=0

echo "Layer-2 Rust builtin boundary audit (Issue #7878):"
echo "  BuiltinId variants: $current_count (baseline $BASELINE_BUILTIN_COUNT)"
echo "  Layer-2 LOC:        $loc_total (baseline $BASELINE_BUILTIN_LOC, tolerance +$LOC_TOLERANCE)"
echo ""

if [[ "$current_count" -gt "$BASELINE_BUILTIN_COUNT" ]]; then
    echo "ERROR: BuiltinId variant count grew: $current_count > baseline $BASELINE_BUILTIN_COUNT."
    echo "       A new domain builtin was added to the Layer-2 Rust surface."
    echo "       Justify it against RUST_BOUNDARY_JUSTIFICATION.md condition 1-4 with a"
    echo "       '// Boundary: condition N (...), Issue #NNNN' comment and bump"
    echo "       BASELINE_BUILTIN_COUNT in this script, OR implement it in Pure Julia."
    failed=1
fi

loc_ceiling=$((BASELINE_BUILTIN_LOC + LOC_TOLERANCE))
if [[ "$loc_total" -gt "$loc_ceiling" ]]; then
    echo "ERROR: Layer-2 LOC grew: $loc_total > baseline+tolerance $loc_ceiling."
    echo "       The Rust builtin surface expanded substantially. Justify the new"
    echo "       domain logic against RUST_BOUNDARY_JUSTIFICATION.md condition 1-4 and"
    echo "       bump BASELINE_BUILTIN_LOC in this script (cite the Issue), OR move the"
    echo "       logic to Pure Julia (subset_julia_vm/src/julia/)."
    failed=1
fi

if [[ "$failed" -ne 0 ]]; then
    echo ""
    echo "FAILED: Layer-2 Rust builtin surface grew without an updated baseline."
    echo "See docs/COMPARISION.md (P4) and docs/vm/RUST_BOUNDARY_JUSTIFICATION.md."
    exit 1
fi

# Informational: ratchet-down reminder (non-fatal).
if [[ "$current_count" -lt "$BASELINE_BUILTIN_COUNT" || "$loc_total" -lt "$BASELINE_BUILTIN_LOC" ]]; then
    echo "NOTE: the Layer-2 surface shrank below baseline (Pure Julia migration progress)."
    echo "      Consider lowering BASELINE_BUILTIN_COUNT/BASELINE_BUILTIN_LOC to tighten"
    echo "      the ratchet (current: count=$current_count, loc=$loc_total)."
    echo ""
fi

echo "OK: no new domain-logic Rust builtins beyond baseline."
exit 0
