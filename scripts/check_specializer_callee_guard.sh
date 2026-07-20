#!/usr/bin/env bash
# check_specializer_callee_guard.sh
#
# Issue #10418 (prevention for Issue #10146) — audit that the VM runtime
# specializer's `compile_call` (subset_julia_vm_vm/src/vm/specialize/expr.rs)
# checks LOCAL bindings before matching any name-keyed builtin fast path.
#
# Background: the specializer used to match name-keyed arms ("Float64",
# "Int64", "sqrt", "round", ...) before checking whether the callee name was
# a local binding, so a parameter named `Float64` was compiled as the builtin
# constructor inside specialized bodies (Issue #10146, fixed by PR #10417
# with a front-door local-callee guard). This audit fails when:
#   1. `fn compile_call` can no longer be located (rename/split refactor —
#      update this script together with the refactor), or
#   2. the local-callee guard `self.locals.contains_key(function)` is
#      missing, or
#   3. a name-keyed callee dispatch (`match function`, `function ==`,
#      `function.starts_with`, `function.ends_with`) appears BEFORE the
#      guard inside the function body.
#
# Companion tests:
#   vm::specialize::tests::test_issue_10418_local_callee_shadowing_matrix_over_specializer_fast_paths
#   subset_julia_vm/tests/fixtures/functions/parameter_shadows_numeric_constructor_10146.jl
# Checklist: docs/vm/CHECKLISTS.md — "Runtime Specializer Name-Keyed Callee
# Fast Paths (Issue #10418)".
#
# Usage: run from the repository root
#   bash scripts/check_specializer_callee_guard.sh
#
# Exit code: 0 = OK, 1 = guard missing/misordered (or checker self-test
# failure).

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

EXPR_RS="subset_julia_vm_vm/src/vm/specialize/expr.rs"

# Print the body of `fn compile_call(` from its signature line up to (not
# including) the next same-indent `fn` item, with whole-line `//` comments
# stripped so commentary cannot trip the name-keyed dispatch patterns.
extract_compile_call() {
    awk '
        /^    (pub(\([a-z: ]*\))? )?fn compile_call\(/ { infn = 1; print; next }
        infn && /^    (pub(\([a-z: ]*\))? )?fn [A-Za-z_]/ { exit }
        infn { print }
    ' "$1" | grep -v '^[[:space:]]*//' || true
}

# check_body BODY LABEL — enforce the guard-before-name-keyed-arms ordering
# invariant on an extracted `compile_call` body.
check_body() {
    local body="$1" label="$2"
    local guard_line namekey_line

    if [[ -z "$body" ]]; then
        echo "ERROR($label): could not locate fn compile_call. If it was renamed or split, update scripts/check_specializer_callee_guard.sh in the same PR (Issue #10418)."
        return 1
    fi

    guard_line=$(printf '%s\n' "$body" | grep -n 'locals\.contains_key(function)' | head -n 1 | cut -d: -f1) || true
    namekey_line=$(printf '%s\n' "$body" | grep -nE 'match function|function ==|function\.starts_with|function\.ends_with' | head -n 1 | cut -d: -f1) || true

    if [[ -z "$guard_line" ]]; then
        echo "ERROR($label): compile_call lost the front-door local-callee guard 'self.locals.contains_key(function)' (PR #10417)."
        echo "A local binding in callee position must shadow every name-keyed builtin fast path (Issue #10146)."
        return 1
    fi
    if [[ -n "$namekey_line" && "$namekey_line" -lt "$guard_line" ]]; then
        echo "ERROR($label): a name-keyed callee dispatch appears BEFORE the local-callee guard (comment-stripped body line $namekey_line < guard line $guard_line)."
        echo "Every name-keyed specializer fast path must run AFTER the guard, or explicitly prove the name is not locally bound (Issue #10418; docs/vm/CHECKLISTS.md 'Runtime Specializer Name-Keyed Callee Fast Paths')."
        return 1
    fi
    return 0
}

# Negative + positive self-test so the checker itself cannot silently rot
# (audit-script negative-test policy, docs/vm/CODE_AUDITS.md).
self_test() {
    local bad good
    bad='    pub(super) fn compile_call(
        match function {
            "sqrt" => {}
        }
        if self.locals.contains_key(function) {}
    }'
    if check_body "$bad" "self-test/bad" >/dev/null; then
        echo "ERROR: checker self-test failed — a name-keyed arm placed before the guard was not flagged."
        return 1
    fi
    good='    pub(super) fn compile_call(
        if self.locals.contains_key(function) {}
        match function {
            "sqrt" => {}
        }
    }'
    if ! check_body "$good" "self-test/good" >/dev/null; then
        echo "ERROR: checker self-test failed — a compliant guard-first body was flagged."
        return 1
    fi
    if check_body "" "self-test/empty" >/dev/null; then
        echo "ERROR: checker self-test failed — an empty extraction (compile_call not found) was not flagged."
        return 1
    fi
    return 0
}

if [[ ! -f "$EXPR_RS" ]]; then
    echo "ERROR: $EXPR_RS not found. Run this script from the repository root."
    exit 1
fi

self_test

body=$(extract_compile_call "$EXPR_RS")
check_body "$body" "$EXPR_RS"

echo "OK: the runtime specializer's compile_call checks local bindings before every name-keyed builtin fast path in $EXPR_RS (Issues #10146/#10418)."
