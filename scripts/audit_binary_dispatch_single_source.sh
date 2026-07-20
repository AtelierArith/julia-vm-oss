#!/usr/bin/env bash
# audit_binary_dispatch_single_source.sh — single-source ratchet for binary dispatch
#
# Verifies that the resolver adapter in `compile_binary_op` covers all expected
# binary operators and that the compare-mode annotations are in place.  Acts as
# a CI ratchet: fails if key patterns are missing so that future refactors cannot
# silently remove the adapter coverage.
#
# Part of the binary dispatch unification track (Issue #8622, parent #8609).
#
# Usage:
#   bash scripts/audit_binary_dispatch_single_source.sh
#   # exit 0 = ok, exit 1 = missing coverage

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MOD_FILE="$REPO_ROOT/subset_julia_vm_compile/src/compile/expr/binary/mod.rs"
BUILTIN_FILE="$REPO_ROOT/subset_julia_vm_compile/src/compile/expr/binary/builtin.rs"
# dispatch_resolver.rs moved to the types crate in the crate split; the old
# subset_julia_vm/src/inference_core path made the resolver anchors red on the
# clean tree (Issue #9573).
RESOLVER_FILE="$REPO_ROOT/subset_julia_vm_types/src/inference_core/dispatch_resolver.rs"

pass=0
fail=0

# Guard against path drift (Issue #9573): fail with an explicit diagnostic when
# a hardcoded target file is gone, instead of per-check grep noise.
for target in "$MOD_FILE" "$BUILTIN_FILE" "$RESOLVER_FILE"; do
    if [[ ! -f "$target" ]]; then
        echo "ERROR: audit target file missing: $target (moved/removed by a refactor? Repoint this audit — Issue #9573)."
        fail=$((fail + 1))
    fi
done
if [[ $fail -gt 0 ]]; then
    echo ""
    echo "Results: $pass passed, $fail failed"
    exit 1
fi

check() {
    local label="$1"
    local file="$2"
    local pattern="$3"
    if grep -q "$pattern" "$file"; then
        echo "OK  $label"
        pass=$((pass + 1))
    else
        echo "FAIL $label"
        echo "     Expected pattern: $pattern"
        echo "     In file: $file"
        fail=$((fail + 1))
    fi
}

echo "=== Binary dispatch single-source audit (Issue #8622) ==="
echo ""

# 1. Resolver functions must be declared in dispatch_resolver.rs
check "binary_dispatch_compare_enabled declared" \
    "$RESOLVER_FILE" \
    "pub fn binary_dispatch_compare_enabled"

check "binary_dispatch_compare_log declared" \
    "$RESOLVER_FILE" \
    "pub fn binary_dispatch_compare_log"

check "binary_static_verdict declared in dispatch_resolver" \
    "$RESOLVER_FILE" \
    "pub fn binary_static_verdict"

check "BinaryStaticVerdict enum declared" \
    "$RESOLVER_FILE" \
    "pub enum BinaryStaticVerdict"

# 2. The resolver adapter must cover all arithmetic operators in compile_binary_op
check "resolver adapter covers Add" \
    "$MOD_FILE" \
    "resolver_overrides_to_builtin"

check "resolver adapter includes Eq (comparison #8622)" \
    "$MOD_FILE" \
    "BinaryOp::Eq.*// Issue #8622"

check "resolver adapter includes Ge" \
    "$MOD_FILE" \
    "BinaryOp::Ge"

# 3. Compare-mode annotations must be present at all instrumented call sites
check "compare-mode annotation in needs_runtime_dispatch path" \
    "$MOD_FILE" \
    'binary_compare_check.*"NeedsRuntime"'

check "compare-mode annotation in has_any path" \
    "$MOD_FILE" \
    'binary_compare_check.*left_ty.*"NeedsRuntime"'

check "compare-mode annotation in main numeric path (UniqueBuiltin)" \
    "$MOD_FILE" \
    'binary_compare_check.*left_ty.*"UniqueBuiltin"'

check "compare-mode annotation in compile_builtin_binary_op (UniqueBuiltin)" \
    "$BUILTIN_FILE" \
    'binary_compare_check.*"UniqueBuiltin"'

# 4. binary_compare_check function must use the bridge
# (the LatticeType::from impl became runtime_types::bridge::value_type_to_lattice
#  — Issue #9573 repoint)
check "binary_compare_check uses value_type_to_lattice bridge" \
    "$MOD_FILE" \
    "value_type_to_lattice(left_vt)"

# 5. Sweep script must exist and be executable
SWEEP_SCRIPT="$REPO_ROOT/scripts/sweep_binary_dispatch_compare.sh"
if [[ -x "$SWEEP_SCRIPT" ]]; then
    echo "OK  sweep_binary_dispatch_compare.sh exists and is executable"
    pass=$((pass + 1))
else
    echo "FAIL sweep_binary_dispatch_compare.sh missing or not executable"
    fail=$((fail + 1))
fi

echo ""
echo "Results: $pass passed, $fail failed"
if [[ $fail -gt 0 ]]; then
    exit 1
fi
exit 0
