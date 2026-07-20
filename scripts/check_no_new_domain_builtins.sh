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
# Layer-2 Rust builtin surface (`subset_julia_vm_bytecode/src/builtins.rs` +
# `subset_julia_vm_vm/src/vm/builtins_*.rs`) and FAILS when either grows past its
# baseline:
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
# Bumped 2026-07-02 for `BuiltinId::EvalDefinedCall`, the compiler-internal
# trampoline body a runtime-`eval`-defined method installs (Issue #8647,
# Boundary condition 3: eval-internal representation).
# Bumped 2026-07-08 for `BuiltinId::ComplexScaleTpRange` (+48 LOC handler,
# Issue #9659, Boundary condition 3: TwicePrecision hi/lo of native ranges are
# VM-internal representation). NOTE: main had already drifted to ~11747 LOC
# (125 over the old ceiling) without a recorded bump before this measurement —
# the re-baseline absorbs that drift; see the #9659 PR discussion.
# Bumped 2026-07-08 for the crate-split path correction,
# `BuiltinId::BroadcastTypedKernel` + `BuiltinId::BroadcastBinaryArith`
# (Issues #9693/#8797, Boundary condition 4: no-JIT broadcast performance
# boundary), and `BuiltinId::_TypeEqual` (Issue #9563, Boundary condition 3:
# runtime type-object equality depends on VM-internal DataType/UnionAll/TypeVar
# representation).
# Re-baselined 2026-07-09 for the observed 293-variant surface and the
# milestone-62 `Base.promote_op` Complex type-metadata reflection refinement
# (Issue #9835, Boundary condition 3: VM-internal type representation).
# Re-baselined 2026-07-10 (Issue #9696 drift attribution — the recorded-bump
# procedure for +11 variants / +340 LOC that landed since the 2026-07-09
# baseline commit 30ca274c5 without a bump; per-Issue breakdown, each with a
# retro-added `// Boundary:` comment in builtins.rs where a variant was added):
#   +8 variants (WeakRefNew/Value/SetValue, Finalizer, Finalize, GcCollect,
#      GcSafepoint, GcInFinalizer) +115 LOC — PR #10132, Issue #8990,
#      condition 3 (VM Rc memory-management internals).
#   +3 variants (PipeNew, RedirectStdout, RedirectStderr) +132 LOC —
#      PR #10041, Issues #9577/#10034, condition 1 (process stdio boundary).
#   +34 LOC builtins_io.rs — PR #10007, Issue #9578 (binary write raw-byte
#      semantics; existing IOWrite handler, condition 1).
#   +45 LOC builtins_collections.rs — PR #9912, Issues #9920/#9921 (string-
#      dispatch debt: eltype check extracted to an audited helper + ratchets).
#   +14 LOC — PR #10116, Issue #9518 (exact BigInt range endpoints, condition 2
#      GMP-backed values in existing handlers).
#   +3 LOC — PR #10069, Issue #10019; -11 LOC — PR #9984 Issue #9741 + crate-
#      split routing (#9090). Net +332, +8 LOC of Boundary comments (#9696).
# Corrected 2026-07-10 for pre-existing `BuiltinId::IsPublic` (Issue #876),
# already classified as condition 3 module binding table reflection but omitted
# from the #9696 recorded count.
# Also re-baselined 2026-07-10 (Issue #10247 drift attribution — commit
# df0135a6f / PR #10067 added `BuiltinId::IsdefinedBindingField`, Core.Binding
# UndefRefError semantics, condition 3: VM-internal binding representation,
# without bumping this ratchet; discovered while lead-merging milestone #69
# CompileSpeed, whose own cluster PRs do not touch builtins.rs).
# These two corrections landed independently on separate branches and were
# merged together here; the actual measured `BuiltinId` count on the merged
# tree (both `IsPublic` and `IsdefinedBindingField` present) is 305, not the
# 306 a naive "+1 +1" from the recorded 304 would suggest — set directly from
# the merged tree's real count rather than assumed arithmetic.
# Re-baselined 2026-07-11 (Issues #10247/#10256 drift attribution — commit
# b191baf8e / PR #10224 added `BuiltinId::TestRecordError` (errored @test
# outcome recorder, condition 3: testset counters/summary/exit flag are
# VM-internal session state) without bumping this ratchet; a second instance
# of the exact #10247 class, caught while wiring this audit plus the schema-
# fingerprint audit into scripts/premerge_gate.sh's default gates, #10256):
#   +1 variant (TestRecordError) — PR #10224, Issue #10093.
# Re-baselined 2026-07-11 (Issue #10440 — a third instance of the #10247
# drift class: several unrelated milestone PRs landed on `main` in a short,
# highly concurrent window without bumping this ratchet, pushing Layer-2 LOC
# to 14773 with the `BuiltinId` variant count unchanged at 306 — i.e. no new
# domain-logic shortcuts, just comment/handler-body growth in already-
# justified existing builtins. Absorbing the drift directly since no single
# attributable commit/Issue could be pinned given the churn volume; caught
# while lead-merging milestone #73 whose own cluster PRs do not touch
# builtins.rs).
# Re-baselined 2026-07-12 for Issue #10631 after tuple equality was routed
# through the existing heap-aware array-like equality handler in
# `builtins_equality.rs`. No BuiltinId variant was added; this is LOC growth in
# an existing condition-3 VM-internal equality boundary that main had already
# merged before the ratchet baseline was updated.
# Re-baselined 2026-07-12 for Issues #10748/#10752 after current `origin/main`
# already contained three additional regex/string helper BuiltinId variants
# without the matching ratchet bump. The measured merged-tree count is 309;
# LOC remained inside the existing tolerance band. The Issue #10704 branch adds
# no BuiltinId variants of its own.
# Bumped 2026-07-13 for Issue #10349: +6 Task continuation boundary variants
# and +465 audited Layer-2 LOC (`builtins_tasks.rs` + enum/dispatch wiring).
# These are condition-3 VM-internal frame/stack/session transfers; all public
# Task, Channel, Condition, and scheduler semantics remain in Pure Julia.
# Re-baselined 2026-07-15 for Issue #10460 after source `where` binders were
# rebound into identity-bearing runtime UnionAll graphs. No BuiltinId variant
# was added; the growth is condition-3 VM-internal type-object construction,
# nested-binder identity, and lexical qualification handling that cannot live
# in Pure Julia without the runtime TypeVar IDs/CoreType graph.
# 316 (Issue #9509): SteprangelenF64 (`_steprangelen_range_f64`) — condition-3
# TwicePrecision VM-internal range representation for
# `range(start; step, length)`, the same boundary as LinspaceF64 (#9419).
# 317 (Issue #11171): _ModuleName (`_module_name`) — condition-3 module-name
# reflection over the VM-internal ModuleValue representation (same boundary
# as IsPublic/IsdefinedModuleBinding); public surface is the Pure Julia
# `nameof(m::Module)` wrapper.
# 318: BuiltinId::ThrowMethodErrorWithArgs (Issue #11374) — the compile-time
# dispatch-miss raise keeps its argument values so a caught MethodError
# exposes upstream's .f/.args; not a domain builtin (error-funnel plumbing).
BASELINE_BUILTIN_COUNT=318
BASELINE_BUILTIN_LOC=16689
# LOC tolerance absorbs comment/format churn while still catching real growth
# (a large new match arm in an existing handler that adds no new BuiltinId).
LOC_TOLERANCE=300
# -----------------------------------------------------------------------------

BUILTINS_RS="subset_julia_vm_bytecode/src/builtins.rs"
VM_DIR="subset_julia_vm_vm/src/vm"

if [[ ! -f "$BUILTINS_RS" ]]; then
    echo "ERROR: $BUILTINS_RS not found. Run from the repository root." >&2
    exit 1
fi

# ---- Metric 1: BuiltinId enum variant count ---------------------------------
# Extract identifiers declared inside `pub enum BuiltinId { ... }`, skipping
# comment lines, and count distinct variants.
current_variants=$(
    awk '/pub enum BuiltinId \{/{f=1;next} f&&/^\}/{f=0} f' "$BUILTINS_RS" \
        | grep -vE '^[[:space:]]*//' \
        | grep -oE '^[[:space:]]*[A-Z_][A-Za-z0-9_]*' \
        | sed 's/^[[:space:]]*//' \
        | sort -u \
        || true
)
current_count=$(printf '%s\n' "$current_variants" | grep -c . || true)
current_count=${current_count//[^0-9]/}
: "${current_count:=0}"

# ---- Metric 2: Layer-2 total LOC --------------------------------------------
# The per-file breakdown is kept so the LOC-ratchet failure can name WHICH file
# grew — an actionable diagnostic, and an injection-specific reason for the
# negative self-test (a clean tree can never print an injected filename;
# Issue #9892).
loc_total=0
loc_breakdown=""
for f in "$BUILTINS_RS" "$VM_DIR"/builtins_*.rs; do
    [[ -f "$f" ]] || continue
    n=$(wc -l < "$f" | tr -d ' ')
    loc_total=$((loc_total + n))
    loc_breakdown="${loc_breakdown}$(printf '         - %6d  %s' "$n" "$f")
"
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
    echo "       Current BuiltinId variants:"
    printf '%s\n' "$current_variants" | sed 's/^/         - /'
    failed=1
fi

loc_ceiling=$((BASELINE_BUILTIN_LOC + LOC_TOLERANCE))
if [[ "$loc_total" -gt "$loc_ceiling" ]]; then
    echo "ERROR: Layer-2 LOC grew: $loc_total > baseline+tolerance $loc_ceiling."
    echo "       The Rust builtin surface expanded substantially. Justify the new"
    echo "       domain logic against RUST_BOUNDARY_JUSTIFICATION.md condition 1-4 and"
    echo "       bump BASELINE_BUILTIN_LOC in this script (cite the Issue), OR move the"
    echo "       logic to Pure Julia (subset_julia_vm/src/julia/)."
    echo "       Per-file LOC (compare against the previous run to find what grew):"
    printf '%s' "$loc_breakdown"
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
