#!/usr/bin/env bash
# check_call_function_variable_value_dispatch_order.sh — Prevention for
# Issue #9987 ("Prevention: keep function-value dispatch on value-based
# resolver").
#
# Root cause (already fixed on main; this is the regression guard): the
# `Instr::CallFunctionVariable` / `Instr::CallFunctionVariableWithKwargsSplat`
# / `Instr::CallFunctionVariableWithSplat` execution arms in
# subset_julia_vm_vm/src/vm/exec/call_function_variable.rs must route through
# `dispatch_function_variable_for_values`. That shared semantic resolver owns
# the VALUE-BASED runtime dispatcher (`find_best_method_index_from_candidates`)
# and only uses the legacy string scorer (`self.dispatch_function_variable`)
# after a value-based miss or at the explicit parametric-constructor migration
# bridge. Function values
# such as `f = Base.map`
# or kwargs-splat callable calls like `plot(cos, xs)` carry only coarse
# call-site type names, so several `(::Any, ::Any)`-shaped methods tie under
# the string scorer and the VM can select a lazy iterator shim or a generic
# fallback instead of the Julia-specific callable method (Issues #9974,
# #9981).
#
# This is a structural (text-order) audit, not a runtime test: it is cheap,
# deterministic, and pins the SOURCE ORDER of the two dispatch attempts inside
# each of the three `Instr::CallFunctionVariable*` match arms, and pins the
# value-before-legacy order inside the shared resolver. A future edit that
# reintroduces a local scorer or reorders the shared resolver fails here
# instead of silently regressing runtime HOF/
# package dispatch (only reproducible in the full fixture suite, as #9979/
# #9981 were).
#
# Scope: the three value-driven `Instr::CallFunctionVariable*` opcodes named in
# Issue #9987's blast radius, the four declared-signature
# `Instr::InvokeFunctionVariable*` lanes from Issue #11619, and direct dynamic
# calls unified under Issue #10461. `Instr::CallGlobalRef` remains out of scope.
#
# Adding a NEW dynamic callable opcode? See the CHECKLISTS.md rule this audit
# enforces: call `dispatch_function_variable_for_values`; do not reproduce its
# value-based/legacy fallback policy in the opcode arm.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$ROOT/subset_julia_vm_vm/src/vm/exec/call_function_variable.rs"
TARGET_DYNAMIC="$ROOT/subset_julia_vm_vm/src/vm/exec/call_dynamic.rs"
TARGET_COMPILER="$ROOT/subset_julia_vm_compile/src/compile/core_compiler.rs"
TARGET_COMPILE_EXPR="$ROOT/subset_julia_vm_compile/src/compile/expr"
TARGET_OPERANDS="$ROOT/subset_julia_vm_bytecode/src/operands.rs"

for target in "$TARGET" "$TARGET_DYNAMIC" "$TARGET_COMPILER" "$TARGET_OPERANDS"; do
    if [[ ! -f "$target" ]]; then
        echo "ERROR: $target missing — update check_call_function_variable_value_dispatch_order.sh (Issue #10461)."
        exit 1
    fi
done

# awk state machine: track the CURRENT `Instr::CallFunctionVariable*` arm (if
# any). An arm starts at a `            Instr::CallFunctionVariable...` line
# (12-space indent, matching every sibling arm of the `match instr` block in
# `execute_call_function_variable`) and ends at the next sibling `Instr::`
# arm, the trailing catch-all `_ => Err(super::unhandled(instr))`, or EOF.
#
# Within an active arm, require the shared resolver and reject either private
# scorer. The second awk pass checks that the shared resolver itself contains
# the value-based scorer before its legacy fallback.
awk '
BEGIN { arm = ""; arm_start = 0; shared_line = 0; fbm_line = 0; dfv_line = 0; errors = 0; arms_seen = 0 }

function evaluate() {
    if (shared_line == 0) {
        print "ERROR: arm at line " arm_start " (" arm ") never calls" \
              " dispatch_function_variable_for_values() — dynamic callable" \
              " dispatch bypasses the shared semantic resolver" \
              " (Issue #9987)."
        errors++
    }
    if (fbm_line != 0) {
        print "ERROR: arm at line " arm_start " (" arm ") calls" \
              " find_best_method_index_from_candidates() directly at line " \
              fbm_line " — value selection belongs in" \
              " dispatch_function_variable_for_values() (Issue #10461)."
        errors++
    }
    if (dfv_line != 0) {
        print "ERROR: arm at line " arm_start " (" arm ") calls" \
              " self.dispatch_function_variable() directly at line " dfv_line \
              " — legacy scoring belongs behind the shared semantic resolver" \
              " (Issue #10461)."
        errors++
    }
}

/^            Instr::CallFunctionVariable/ {
    if (arm != "") { evaluate() }
    arms_seen++
    arm = $0; arm_start = NR; shared_line = 0; fbm_line = 0; dfv_line = 0
    next
}

/^            Instr::/ {
    if (arm != "") { evaluate() }
    arm = ""
    next
}

/^            _ => Err\(super::unhandled\(instr\)\)/ {
    if (arm != "") { evaluate() }
    arm = ""
    next
}

{
    if (arm != "") {
        code = $0
        sub(/^[ \t]*/, "", code)
        if (code !~ /^\/\// && shared_line == 0 && code ~ /self\.dispatch_function_variable_for_values\(/) { shared_line = NR }
        if (code !~ /^\/\// && fbm_line == 0 && code ~ /self\.find_best_method_index_from_candidates\(/) { fbm_line = NR }
        if (code !~ /^\/\// && dfv_line == 0 && code ~ /self\.dispatch_function_variable\(/) { dfv_line = NR }
    }
}

END {
    if (arm != "") { evaluate() }
    if (arms_seen != 3) {
        print "ERROR: expected exactly 3 Instr::CallFunctionVariable* arms," \
              " found " arms_seen " — update the audit for the opcode shape" \
              " (Issue #10461)."
        errors++
    }
    if (errors > 0) {
        print "FAILED: check_call_function_variable_value_dispatch_order.sh (Issue #9987)."
        exit 1
    }
    print "OK: every Instr::CallFunctionVariable* arm uses the shared" \
          " dispatch_function_variable_for_values() resolver (Issues #9987/#10461)."
}
' "$TARGET"

# `invoke` is deliberately NOT value-driven: its declared tuple is complete
# semantic input, including literal `Any`. All four static/dynamic ×
# positional/keyword opcode forms must route through the declared-signature
# helper and must not call the ordinary value-based resolver directly (Issue
# #11619).
awk '
BEGIN { arm = ""; arm_start = 0; route_line = 0; values_line = 0; legacy_line = 0; errors = 0; arms_seen = 0 }

function evaluate() {
    if (route_line == 0) {
        print "ERROR: invoke arm at line " arm_start " (" arm ") never calls" \
              " invoke_runtime_callable_value_with_signature*() — declared" \
              " signature mode can drift by opcode lane (Issue #11619)."
        errors++
    }
    if (values_line != 0) {
        print "ERROR: invoke arm at line " arm_start " (" arm ") calls" \
              " dispatch_function_variable_for_values() directly at line " \
              values_line " — invoke must not refine its declared signature" \
              " from runtime values (Issue #11619)."
        errors++
    }
    if (legacy_line != 0) {
        print "ERROR: invoke arm at line " arm_start " (" arm ") calls" \
              " self.dispatch_function_variable() directly at line " legacy_line \
              " — declared-signature dispatch belongs in the shared invoke" \
              " helper (Issue #11619)."
        errors++
    }
}

/^            Instr::InvokeFunctionVariable/ {
    if (arm != "") { evaluate() }
    arms_seen++
    arm = $0; arm_start = NR; route_line = 0; values_line = 0; legacy_line = 0
    next
}

/^            Instr::/ {
    if (arm != "") { evaluate() }
    arm = ""
    next
}

/^            _ => Err\(super::unhandled\(instr\)\)/ {
    if (arm != "") { evaluate() }
    arm = ""
    next
}

{
    if (arm != "") {
        code = $0
        sub(/^[ \t]*/, "", code)
        if (code !~ /^\/\// && route_line == 0 && code ~ /self\.invoke_runtime_callable_value_with_signature(_and_kwargs)?\(/) { route_line = NR }
        if (code !~ /^\/\// && values_line == 0 && code ~ /self\.dispatch_function_variable_for_values\(/) { values_line = NR }
        if (code !~ /^\/\// && legacy_line == 0 && code ~ /self\.dispatch_function_variable\(/) { legacy_line = NR }
    }
}

END {
    if (arm != "") { evaluate() }
    if (arms_seen != 4) {
        print "ERROR: expected exactly 4 Instr::InvokeFunctionVariable* arms," \
              " found " arms_seen " — update the declared-signature lane audit" \
              " (Issue #11619)."
        errors++
    }
    if (errors > 0) {
        print "FAILED: invoke declared-signature routing audit (Issue #11619)."
        exit 1
    }
    print "OK: all four Instr::InvokeFunctionVariable* lanes use the shared" \
          " declared-signature invoke helper (Issue #11619)."
}
' "$TARGET"

awk '
BEGIN { active = 0; start = 0; declared_line = 0; value_line = 0; request_line = 0 }

/^    fn dispatch_function_variable_for_declared_signature\(/ {
    active = 1; start = NR; next
}

active && /^    (pub\([^)]*\) )?fn / { active = 0 }

active {
    code = $0
    sub(/^[ \t]*/, "", code)
    if (code !~ /^\/\// && declared_line == 0 && code ~ /self\.dispatch_function_variable\(func_name, &origin_compatible, declared_arg_type_names\)/) { declared_line = NR }
    if (code !~ /^\/\// && value_line == 0 && code ~ /self\.dispatch_function_variable_for_values\(/) { value_line = NR }
    if (code !~ /^\/\// && request_line == 0 && code ~ /self\.runtime_call_request\(/) { request_line = NR }
}

END {
    if (value_line != 0 || request_line != 0) {
        print "ERROR: declared-signature dispatch must not use value-based runtime" \
              " refinement (Issue #11619)."
        exit 1
    }
    if (start == 0 || declared_line == 0) {
        print "ERROR: dispatch_function_variable_for_declared_signature() must" \
              " select with declared_arg_type_names unchanged (Issue #11619)."
        exit 1
    }
    print "OK: invoke dispatch treats the declared signature as authoritative" \
          " instead of refining it from runtime values (Issue #11619)."
}
' "$TARGET"

awk '
BEGIN { active = 0; start = 0; request_line = 0; resolve_line = 0; fbm_line = 0; dfv_line = 0 }

/^    pub\(in crate::vm\) fn dispatch_function_variable_for_values\(/ {
    active = 1; start = NR; next
}

active && /^    (pub\([^)]*\) )?fn / {
    active = 0
}

active {
    code = $0
    sub(/^[ \t]*/, "", code)
    if (code !~ /^\/\// && request_line == 0 && code ~ /self\.runtime_call_request\(/) { request_line = NR }
    if (code !~ /^\/\// && resolve_line == 0 && code ~ /self\.resolve_runtime_call_request\(/) { resolve_line = NR }
    if (code !~ /^\/\// && fbm_line == 0 && code ~ /self\.find_best_method_index_from_candidates\(/) { fbm_line = NR }
    if (code !~ /^\/\// && dfv_line == 0 && code ~ /self\.dispatch_function_variable\(/) { dfv_line = NR }
}

END {
    if (start == 0 || request_line == 0 || resolve_line == 0 || dfv_line == 0) {
        print "ERROR: dispatch_function_variable_for_values() must build and resolve" \
              " a runtime CallRequest before its legacy miss fallback (Issue #10461)."
        exit 1
    }
    if (fbm_line != 0) {
        print "ERROR: dispatch_function_variable_for_values() calls the value scorer" \
              " directly instead of resolve_runtime_call_request() (Issue #10461)."
        exit 1
    }
    if (resolve_line < request_line || dfv_line < resolve_line) {
        print "ERROR: dispatch_function_variable_for_values() calls legacy scoring" \
              " before request-based value selection (Issue #10461)."
        exit 1
    }
    print "OK: shared callable resolver builds and resolves CallRequest before" \
          " the legacy miss fallback (Issue #10461)."
}
' "$TARGET"

awk '
BEGIN { active = 0; arms_seen = 0; name_line = 0; request_line = 0; resolve_line = 0; direct_line = 0 }

/^            Instr::CallDynamic\(operands\) => \{/ {
    active = 1; arms_seen++; next
}

active && /^            Instr::/ { active = 0 }

active {
    code = $0
    sub(/^[ \t]*/, "", code)
    # The identity-preservation invariant is that the CallRequest carries a
    # CalleeIdentity built FROM operands.callee_name — not merely that the
    # name is referenced somewhere in the arm (a later builtin-fallback use of
    # operands.callee_name must not satisfy this check, Issue #10735).
    if (code !~ /^\/\// && name_line == 0 && code ~ /CalleeIdentity::from_function_name\(&operands\.callee_name\)/) { name_line = NR }
    if (code !~ /^\/\// && request_line == 0 && code ~ /self\.runtime_call_request\(/) { request_line = NR }
    if (code !~ /^\/\// && resolve_line == 0 && code ~ /self\.resolve_runtime_call_request\(/) { resolve_line = NR }
    if (code !~ /^\/\// && direct_line == 0 && code ~ /self\.find_best_method_index_from_candidates\(/) { direct_line = NR }
}

END {
    if (arms_seen != 1 || name_line == 0 || request_line == 0 || resolve_line == 0) {
        print "ERROR: Instr::CallDynamic must consume operands.callee_name and route" \
              " through runtime_call_request()/resolve_runtime_call_request()" \
              " (Issue #10461)."
        exit 1
    }
    if (direct_line != 0) {
        print "ERROR: Instr::CallDynamic calls the value scorer directly at line " \
              direct_line " instead of the shared request resolver (Issue #10461)."
        exit 1
    }
    if (resolve_line < request_line) {
        print "ERROR: Instr::CallDynamic resolves before constructing CallRequest" \
              " (Issue #10461)."
        exit 1
    }
    print "OK: Instr::CallDynamic preserves callee identity and uses the shared" \
          " runtime CallRequest resolver (Issue #10461)."
}
' "$TARGET_DYNAMIC"

if rg -n 'emit\(Instr::CallDynamic\(' "$TARGET_COMPILE_EXPR" >/dev/null; then
    echo "ERROR: compiler expression code emits Instr::CallDynamic directly; use emit_dynamic_call() so callee identity is retained (Issue #10461)."
    exit 1
fi

if ! rg -q 'pub\(in crate::compile\) fn emit_dynamic_call\(' "$TARGET_COMPILER" \
    || ! rg -q 'Instr::call_dynamic\(' "$TARGET_COMPILER" \
    || ! rg -q 'pub callee_name: String' "$TARGET_OPERANDS"; then
    echo "ERROR: dynamic-call compiler/payload identity hub is incomplete (Issue #10461)."
    exit 1
fi

echo "OK: compiler dynamic-call producers use the identity-preserving emission hub (Issue #10461)."
