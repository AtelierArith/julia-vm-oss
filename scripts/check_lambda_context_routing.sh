#!/usr/bin/env bash
# check_lambda_context_routing.sh — LambdaContext routing authority audit
# (Issues #10936, #10965, #11211).
#
# Function-definition lowering must route through ONE central authority —
# `function_lowering_capabilities` in `subset_julia_vm_lowering/src/lowering/mod.rs`
# (consumed via `requires_lambda_context` / `requires_nested_lambda_lowering`
# or the `lower_*_with_ctx_if_needed` helpers). Issue #10934 happened because
# one dispatch site consulted the narrow `contains_macro_call` predicate and
# silently dropped the where-binder edge; Issue #10948's first fix broadened
# `contains_parametrized_type_expression` into closure-lowering mode and broke
# capture analysis in three fixture categories.
#
# Enforced invariants:
#   R1. The narrow structural predicates (`contains_macro_call`,
#       `contains_where_binder`, `contains_parametrized_type_expression`) are
#       referenced ONLY inside the authority file `lowering/mod.rs`. A
#       function-lowering dispatch site elsewhere must use the derived
#       `requires_*` views, never a narrow predicate.
#   R2. Outside `lowering/mod.rs`, the context-less function-definition
#       lowering entries (`function::lower_function_all(`,
#       `function::lower_short_function_all(`,
#       `function::lower_operator_method(`) may appear only on an explicit
#       `None =>` no-context match arm. Any other spelling means a dispatch
#       site is bypassing the routing authority while a live context exists.
#   R3. INSIDE `lowering/mod.rs`, those same context-less entries may appear
#       only within the sanctioned routing helpers
#       (`lower_*_with_ctx_if_needed`), where they are the `else` branch of a
#       real `requires_lambda_context` decision, or on an explicit `None =>`
#       arm. R2 alone is satisfiable by RELOCATING a context-free call into the
#       authority file behind a pass-through wrapper and calling that wrapper
#       from the dispatch site: the grep stops matching, yet the routing
#       authority is never consulted. That shim actually landed (Issue #11179)
#       and silently denied struct-body `global` helpers their macro /
#       where-binder / parametric context — a user macro in such a body failed
#       to lower at all (Issue #11193). R3 makes the bypass unsatisfiable.
#   R4. `Function.new_struct_name` mutation is confined to three structural
#       seams: root struct-helper construction, creation-time lifted-function
#       stamping, and the recursive collector (including the runtime-eval
#       clear). Post-hoc slice/watermark stamping is forbidden.
#   R5. The root, lifted, runtime-eval, and collector seams retain the exact
#       propagation/clear tokens that make the lexical boundary effective.
#   R6. The four struct-new authority regression fixtures remain present and
#       registered together in the struct manifest.
#
# Usage (from the repository root):
#   bash scripts/check_lambda_context_routing.sh
#
# Exit code: 0 = routing confined to the authority, 1 = violation(s) found.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

SRC_DIRS=(
    "subset_julia_vm/src"
    "subset_julia_vm_compile/src"
    "subset_julia_vm_lowering/src"
    "subset_julia_vm_vm/src"
)
AUTHORITY_FILE="subset_julia_vm_lowering/src/lowering/mod.rs"
STRUCT_LOWERING_FILE="subset_julia_vm_lowering/src/lowering/struct_.rs"
EVAL_LOWERING_FILE="subset_julia_vm_lowering/src/lowering/stmt/macros/mod.rs"
COLLECTOR_FILE="subset_julia_vm_compile/src/compile/collect.rs"
STRUCT_FIXTURE_DIR="subset_julia_vm/tests/fixtures/struct"
STRUCT_MANIFEST="$STRUCT_FIXTURE_DIR/manifest.toml"

for src_dir in "${SRC_DIRS[@]}"; do
    if [ ! -d "$src_dir" ]; then
        echo "ERROR: $src_dir not found. Run this script from the repository root." >&2
        exit 1
    fi
done

status=0

# Strip line comments so a predicate named in a doc/comment (e.g. the mutation
# contract's explanation in function/tests.rs) is not a violation.
strip_comments() {
    sed 's://.*$::'
}

# Print one named Rust function through the line before the next function
# declaration. This is intentionally a source-audit boundary, not a Rust
# parser. R5 pins owner-local tokens; R4's complete mutation inventory is the
# rule protected by the Issue #11211 negative mutation.
function_source() {
    local file="$1" target="$2"
    awk -v target="$target" '
        function declaration_name(line, decl) {
            if (match(line, /(^|[^A-Za-z0-9_])fn[ \t]+[A-Za-z_][A-Za-z0-9_]*/)) {
                decl = substr(line, RSTART, RLENGTH)
                sub(/.*fn[ \t]+/, "", decl)
                return decl
            }
            return ""
        }
        {
            name = declaration_name($0)
            if (name != "") {
                if (in_target && name != target) exit 0
                if (name == target) {
                    in_target = 1
                    found = 1
                }
            }
            if (in_target) print
        }
        END { if (!found) exit 1 }
    ' "$file"
}

require_function_tokens() {
    local file="$1" function="$2" token body
    shift 2
    body=$(function_source "$file" "$function") || {
        status=1
        echo "ERROR: lambda_context_routing R5 violation — required authority function $function disappeared from $file (Issue #11211)."
        return
    }
    for token in "$@"; do
        if ! printf '%s\n' "$body" | grep -Fq -- "$token"; then
            status=1
            echo "ERROR: lambda_context_routing R5 violation — $file::$function lost required lexical-authority token: $token (Issue #11211)."
        fi
    done
}

# --- R1: narrow predicates confined to the authority file --------------------
r1_hits=$(grep -rn \
    -e "contains_macro_call" \
    -e "contains_where_binder" \
    -e "contains_parametrized_type_expression" \
    "${SRC_DIRS[@]}" --include="*.rs" \
    | grep -v "^$AUTHORITY_FILE:" \
    | strip_comments \
    | grep -e "contains_macro_call" -e "contains_where_binder" \
           -e "contains_parametrized_type_expression" \
    || true)

if [ -n "$r1_hits" ]; then
    status=1
    echo "ERROR: lambda_context_routing R1 violation — narrow LambdaContext routing predicate used outside the authority (Issues #10936/#10965)."
    echo "Function-lowering dispatch sites must consult function_lowering_capabilities / requires_lambda_context / requires_nested_lambda_lowering in $AUTHORITY_FILE instead."
    echo ""
    echo "$r1_hits"
    echo ""
fi

# --- R2: ctx-less definition entries only on explicit None arms --------------
r2_hits=$(grep -rn \
    -e "function::lower_function_all(" \
    -e "function::lower_short_function_all(" \
    -e "function::lower_operator_method(" \
    "${SRC_DIRS[@]}" --include="*.rs" \
    | grep -v "^$AUTHORITY_FILE:" \
    | strip_comments \
    | grep -e "function::lower_function_all(" \
           -e "function::lower_short_function_all(" \
           -e "function::lower_operator_method(" \
    | grep -v "None => " \
    || true)

if [ -n "$r2_hits" ]; then
    status=1
    echo "ERROR: lambda_context_routing R2 violation — context-less function-definition lowering entry bypasses the routing authority (Issues #10936/#10965)."
    echo "With a live LambdaContext in scope, call crate::lowering::lower_*_with_ctx_if_needed instead; a bare entry is allowed only on an explicit 'None =>' no-context arm."
    echo ""
    echo "$r2_hits"
    echo ""
fi

# --- R3: inside the authority file, ctx-less entries only inside the routing
#         helpers (or on an explicit None arm) -----------------------------------
#
# R2 only greps OUTSIDE the authority file, so it can be satisfied by RELOCATING a
# context-free call INTO `lowering/mod.rs` behind a pass-through wrapper and calling
# that wrapper from the dispatch site — the grep stops matching while the routing
# authority is never consulted. That exact shim landed once (Issue #11179: a
# `lower_struct_global_function_all` whose body was a bare
# `function::lower_function_all(walker, node)`), turning the audit green while
# struct-body `global` helpers silently lost macro/where-binder/parametric context —
# a user macro in such a body could not be lowered AT ALL (Issue #11193).
#
# So: within the authority file itself, a context-free entry may appear ONLY
# - inside one of the sanctioned routing helpers below, where it is the `else`
#   branch of a real `requires_lambda_context` decision, or
# - on an explicit `None =>` no-context match arm.
# A context-free entry in ANY OTHER function in this file is a laundering wrapper.
ROUTING_HELPERS="lower_function_all_with_ctx_if_needed lower_operator_method_with_ctx_if_needed lower_short_function_all_with_ctx_if_needed"

r3_hits=$(strip_comments < "$AUTHORITY_FILE" | awk -v helpers="$ROUTING_HELPERS" '
    BEGIN {
        n = split(helpers, h, " ")
        for (i = 1; i <= n; i++) allowed[h[i]] = 1
    }
    # Track the innermost enclosing `fn <name>` declaration.
    match($0, /(^|[^A-Za-z0-9_])fn[ \t]+[A-Za-z0-9_]+/) {
        decl = substr($0, RSTART, RLENGTH)
        sub(/.*fn[ \t]+/, "", decl)
        cur_fn = decl
    }
    /function::lower_function_all\(|function::lower_short_function_all\(|function::lower_operator_method\(/ {
        if (cur_fn in allowed) next          # legal: the routing helper else-branch
        if ($0 ~ /None => /) next            # legal: explicit no-context arm
        printf "%s:%d: (in fn %s) %s\n", "'"$AUTHORITY_FILE"'", NR, cur_fn, $0
    }
' || true)

if [ -n "$r3_hits" ]; then
    status=1
    echo "ERROR: lambda_context_routing R3 violation — context-less function-definition lowering entry inside the authority file, outside the routing helpers (Issues #10936/#10965/#11179)."
    echo "Relocating a context-free call into $AUTHORITY_FILE behind a pass-through wrapper does NOT satisfy the routing invariant — it only hides it from R2's grep."
    echo "Inside $AUTHORITY_FILE, a context-free entry is allowed only inside one of the routing helpers ($ROUTING_HELPERS), where it is the else-branch of a real requires_lambda_context decision, or on an explicit 'None =>' arm."
    echo ""
    echo "$r3_hits"
    echo ""
fi

# --- R4: new_struct_name writes are confined to structural authorities -------
#
# Compare the complete mutation inventory, not just a count. An added post-hoc
# slice/watermark loop must fail even if all sanctioned assignments remain.
r4_expected=$(mktemp)
r4_actual=$(mktemp)
trap 'rm -f "$r4_expected" "$r4_actual"' EXIT

cat > "$r4_expected" <<'EOF'
subset_julia_vm_compile/src/compile/collect.rs:evaluated.new_struct_name = None;
subset_julia_vm_compile/src/compile/collect.rs:nested.new_struct_name = Some(struct_name.to_string());
subset_julia_vm_lowering/src/lowering/mod.rs:func.new_struct_name = self.active_new_struct_name.borrow().clone();
subset_julia_vm_lowering/src/lowering/struct_.rs:func.new_struct_name = Some(struct_name.to_string());
EOF

grep -rnE '\.new_struct_name[[:space:]]*=' "${SRC_DIRS[@]}" --include='*.rs' \
    | strip_comments \
    | grep -E '\.new_struct_name[[:space:]]*=' \
    | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*/\1:/' \
    | LC_ALL=C sort > "$r4_actual"
LC_ALL=C sort -o "$r4_expected" "$r4_expected"

if ! diff -u "$r4_expected" "$r4_actual"; then
    status=1
    echo "ERROR: lambda_context_routing R4 violation — Function.new_struct_name mutation escaped the root/lifted/collector authorities (Issue #11211)."
    echo "Post-hoc slice or watermark stamping is forbidden; establish authority before lowering and stamp lifted functions in LambdaContext::add_lifted_function."
    echo ""
fi

# --- R5: required lexical-boundary semantics remain in their owning seams ----
require_function_tokens "$AUTHORITY_FILE" lower_struct_global_function_all \
    'with_new_struct_authority(Some(struct_name)' \
    'lower_function_all_with_ctx_if_needed'
require_function_tokens "$AUTHORITY_FILE" lower_struct_global_short_function_all \
    'with_new_struct_authority(Some(struct_name)' \
    'lower_short_function_all_with_ctx_if_needed'
require_function_tokens "$AUTHORITY_FILE" add_lifted_function \
    'if func.new_struct_name.is_none()' \
    'func.new_struct_name = self.active_new_struct_name.borrow().clone();'
require_function_tokens "$STRUCT_LOWERING_FILE" parse_struct_global_helpers \
    'func.new_struct_name = Some(struct_name.to_string());'
require_function_tokens "$EVAL_LOWERING_FILE" lower_eval_function_definition \
    'with_new_struct_authority(None'
require_function_tokens "$COLLECTOR_FILE" collect_stmt_functions_with_new_authority \
    'nested.new_struct_name = Some(struct_name.to_string());' \
    'evaluated.new_struct_name = None;'

# --- R6: semantic fixture family stays registered ----------------------------
fixture_registered() {
    local expected_name="$1" expected_file="$2"
    awk -v expected_name="$expected_name" -v expected_file="$expected_file" '
        function finish_block() {
            if (seen_name && seen_file) found = 1
            seen_name = 0
            seen_file = 0
        }
        /^\[\[tests\]\]$/ { finish_block(); next }
        $0 == "name = \"" expected_name "\"" { seen_name = 1 }
        $0 == "file = \"" expected_file "\"" { seen_file = 1 }
        END {
            finish_block()
            exit(found ? 0 : 1)
        }
    ' "$STRUCT_MANIFEST"
}

for fixture in \
    'struct_global_new_helper_11005|global_new_helper_11005.jl' \
    'struct_ownerless_new_lookup_11204|ownerless_new_lookup_11204.jl' \
    'struct_ownerless_new_keyword_lookup_11204|ownerless_new_keyword_lookup_11204.jl' \
    'struct_ownerless_parametric_new_lookup_11204|ownerless_parametric_new_lookup_11204.jl'
do
    fixture_name=${fixture%%|*}
    fixture_file=${fixture#*|}
    if [ ! -f "$STRUCT_FIXTURE_DIR/$fixture_file" ]; then
        status=1
        echo "ERROR: lambda_context_routing R6 violation — fixture file missing: $STRUCT_FIXTURE_DIR/$fixture_file (Issue #11211)."
    elif ! fixture_registered "$fixture_name" "$fixture_file"; then
        status=1
        echo "ERROR: lambda_context_routing R6 violation — fixture family member is not registered as one struct-manifest entry: $fixture_name -> $fixture_file (Issue #11211)."
    fi
done

if [ "$status" -ne 0 ]; then
    exit 1
fi

echo "OK: LambdaContext routing and struct-new lexical authority are confined to their sanctioned seams (R1-R6, Issue #11211)."
