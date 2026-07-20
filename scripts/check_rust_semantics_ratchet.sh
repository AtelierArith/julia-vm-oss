#!/usr/bin/env bash
# check_rust_semantics_ratchet.sh — monotonic ratchet on the Rust-only
# semantic surface (Issue #8673, parent #8648).
#
# "Pure Julia First" (AGENTS.md principle) says Julia-expressible semantics
# live in subset_julia_vm/src/julia/. scripts/inventory_rust_semantics.sh
# measures how much of the public Julia surface is still implemented ONLY in
# Rust; this gate keeps those numbers monotonically non-increasing so new
# Rust-only surface cannot land silently, and keeps the Issue #8672
# classification (docs/vm/rust_semantics_classification.tsv) complete: every
# BuiltinId and semantic Instr variant must have a classified row.
#
# Ratcheted metrics (from `inventory_rust_semantics.sh --summary`):
#   builtin_public_rust_only        — public builtins with no Pure Julia
#                                     same-name definition
#   rust_semantic_surface_rust_only — distinct public surface functions
#                                     implemented only in Rust
#
# Also ratcheted (Issue #9098):
#   perf_pending_count — rows in rust_semantics_classification.tsv with
#                        category == perf-pending (暫定分類; should reach 0
#                        and stay there once all items are benchmarked and
#                        reclassified as perf-measured or migratable).
#
# On a DECREASE (a migration landed): the script passes and reminds you to
# lower the baseline in the same PR (keep the ratchet tight).
# On an INCREASE: hard failure. Either implement the new surface in Pure
# Julia first (preferred), or justify it against
# docs/vm/RUST_BOUNDARY_JUSTIFICATION.md conditions 1-4, classify it in
# docs/vm/rust_semantics_classification.tsv, and bump the baseline in the
# same commit with that justification.
#
# bash 3.2 compatible (macOS stock). Usage: bash scripts/check_rust_semantics_ratchet.sh

set -euo pipefail

BASELINE_BUILTIN_PUBLIC_RUST_ONLY=95
BASELINE_SURFACE_RUST_ONLY=106
# Issue #9098: all 95 perf-pending items benchmarked and reclassified (2026-07-03).
# This baseline must remain 0 — new perf-pending rows are prohibited unless
# accompanied by a tracking Issue and a time-bounded measurement plan.
BASELINE_PERF_PENDING=0

cd "$(dirname "$0")/.."

if [[ ! -x scripts/inventory_rust_semantics.sh ]]; then
    echo "ERROR: scripts/inventory_rust_semantics.sh not found/executable." >&2
    exit 1
fi

# The summary metrics do not need the julia/ submodule; silence its warning.
summary=$(./scripts/inventory_rust_semantics.sh --summary 2>/dev/null)

metric() {
    # $1 = key; prints the value or empty
    printf '%s\n' "$summary" | sed -n "s/^$1=//p"
}

builtin_rust_only=$(metric builtin_public_rust_only)
surface_rust_only=$(metric rust_semantic_surface_rust_only)
builtin_total=$(metric builtin_total)
instr_semantic_total=$(metric semantic_instr_total)
unclassified=$(metric classified_unclassified)

if [[ -z "$builtin_rust_only" || -z "$surface_rust_only" ]]; then
    echo "ERROR: could not read inventory summary metrics:" >&2
    printf '%s\n' "$summary" >&2
    exit 1
fi

# Count perf-pending rows directly from the TSV (Issue #9098).
# Note: grep -c exits 1 on zero matches (still prints "0"), so we use a
# shell-level fallback assignment rather than embedding || inside $(...),
# which would concatenate the grep output with the echo fallback.
TSV="docs/vm/rust_semantics_classification.tsv"
if [[ -f "$TSV" ]]; then
    perf_pending_count=$(grep -c $'\tperf-pending\t' "$TSV" 2>/dev/null) || perf_pending_count=0
else
    perf_pending_count=0
fi

echo "Rust semantics ratchet (Issue #8673 / #9098):"
echo "  public builtins Rust-only:  $builtin_rust_only (baseline $BASELINE_BUILTIN_PUBLIC_RUST_ONLY)"
echo "  surface functions Rust-only: $surface_rust_only (baseline $BASELINE_SURFACE_RUST_ONLY)"
echo "  perf-pending (暫定分類):     $perf_pending_count (baseline $BASELINE_PERF_PENDING)"

fail=0

if [[ "$builtin_rust_only" -gt "$BASELINE_BUILTIN_PUBLIC_RUST_ONLY" ]]; then
    echo "ERROR: Rust-only public builtin count grew: $builtin_rust_only > baseline $BASELINE_BUILTIN_PUBLIC_RUST_ONLY." >&2
    echo "       Pure Julia First: define the public name in subset_julia_vm/src/julia/ (dispatch-first)" >&2
    echo "       or justify the Rust-only surface against RUST_BOUNDARY_JUSTIFICATION.md conditions 1-4," >&2
    echo "       classify it in docs/vm/rust_semantics_classification.tsv, and bump the baseline in $0." >&2
    fail=1
fi

if [[ "$surface_rust_only" -gt "$BASELINE_SURFACE_RUST_ONLY" ]]; then
    echo "ERROR: Rust-only surface function count grew: $surface_rust_only > baseline $BASELINE_SURFACE_RUST_ONLY." >&2
    echo "       See the message above — same remediation." >&2
    fail=1
fi

if [[ "$perf_pending_count" -gt "$BASELINE_PERF_PENDING" ]]; then
    echo "ERROR: perf-pending row count grew: $perf_pending_count > baseline $BASELINE_PERF_PENDING." >&2
    echo "       New perf-pending rows are prohibited. Either:" >&2
    echo "       (a) benchmark the instruction family and reclassify as perf-measured (condition 4)" >&2
    echo "           or migratable — see Issue #9098 for the per-family bench harness pattern; or" >&2
    echo "       (b) if measurement is deferred, open a tracking Issue, set a deadline, and add the" >&2
    echo "           Issue reference to the evidence column before merging." >&2
    fail=1
fi

# Classification completeness: every inventoried item must be classified
# (Issue #8672). classified_unclassified only appears when rows are missing.
if [[ -n "$unclassified" && "$unclassified" != "0" ]]; then
    echo "ERROR: $unclassified inventory item(s) have no row in docs/vm/rust_semantics_classification.tsv." >&2
    echo "       Add a classified row (category + evidence) for every new BuiltinId / semantic Instr variant." >&2
    fail=1
fi
if [[ -z "$(metric classified_migratable)" ]]; then
    echo "ERROR: classification join produced no classified_* metrics — is docs/vm/rust_semantics_classification.tsv present?" >&2
    fail=1
fi

if [[ "$fail" -ne 0 ]]; then
    exit 1
fi

if [[ "$builtin_rust_only" -lt "$BASELINE_BUILTIN_PUBLIC_RUST_ONLY" || "$surface_rust_only" -lt "$BASELINE_SURFACE_RUST_ONLY" ]]; then
    echo "NOTE: metrics improved below baseline — lower the BASELINE_* values in $0 in this PR to lock in the progress."
fi
if [[ "$perf_pending_count" -lt "$BASELINE_PERF_PENDING" ]]; then
    echo "NOTE: perf-pending count improved below baseline — lower BASELINE_PERF_PENDING in $0 to lock in the progress."
fi

echo "OK: Rust-only semantic surface within baseline ($builtin_total builtins / $instr_semantic_total semantic instrs inventoried, all classified; $perf_pending_count perf-pending)."
