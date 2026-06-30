#!/usr/bin/env bash
# Issue #6694 (prevention for the StructRef value-op family
# #6685 tuple-`==` / #6691 `in` / #6693 hash / #6709 `===`).
#
# Immutable structs (OneTo, UnitRange, ordinary non-`mutable` user structs) are
# stored on the struct heap as `Value::StructRef(idx)`. Any native VM op that
# compares or hashes a `Value` by STRUCTURE — `==` over tuples/named-tuples
# (`TupleEquals`), `hash`/`_hash`, `in`/`∈` membership, and `===` over immutable
# structs (`Egal`) — must first resolve those heap refs to inline snapshots,
# otherwise it compares/hashes heap indices and two separately-constructed but
# equal values are reported unequal (or hash differently).
#
# The canonical resolvers live in subset_julia_vm/src/vm/builtins_equality.rs:
#   - resolve_value_op_structrefs(value, heap)      (resolve ALL structs: ==, hash, in)
#   - resolved_value_op_structrefs(&value, heap)    (borrowing Cow variant)
#   - self.resolve_immutable_structrefs(&value, ..) (resolve IMMUTABLE only: ===)
#   - values_equal_for_membership(a, b, heap)       (membership wrapper)
#
# This audit FAILS if any of the structural value-op handler arms stops calling a
# resolver, so the resolution can't be silently dropped from one of them again.
#
# Adding a NEW native structural compare/hash op? Route its operands through one
# of the resolvers above and add its `BuiltinId::` arm to REQUIRED_HANDLERS.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EQ="$ROOT/subset_julia_vm/src/vm/builtins_equality.rs"
TYPES="$ROOT/subset_julia_vm/src/vm/builtins_types.rs"

errors=0

if [[ ! -f "$EQ" ]]; then
    echo "ERROR: $EQ missing — update check_native_value_ops_resolve_structref.sh (Issue #6694)."
    exit 1
fi

# 1. The canonical resolver helpers must still exist (anchor so a rename can't
#    make the per-handler check silently pass with no resolver to find).
for anchor in \
    "fn resolve_value_op_structrefs" \
    "fn resolved_value_op_structrefs" \
    "fn resolve_immutable_structrefs" \
    "fn values_equal_for_membership"; do
    if ! grep -q "$anchor" "$EQ"; then
        echo "ERROR: resolver '$anchor' is gone from builtins_equality.rs (Issue #6694)."
        echo "       Update this audit if it was renamed."
        errors=$((errors + 1))
    fi
done

# 2. Each structural value-op handler arm must call a StructRef resolver. Scan
#    each `BuiltinId::NAME =>` arm body up to the next `BuiltinId::` arm.
REQUIRED_HANDLERS="Egal TupleEquals Hash _Hash"

for handler in $REQUIRED_HANDLERS; do
    found=$(awk -v target="BuiltinId::${handler} =>" '
        index($0, target) { inblock = 1; resolved = 0; next }
        inblock && /BuiltinId::[A-Za-z_]+ =>/ {
            # reached the next arm: report result for the block just closed
            print resolved
            inblock = 0
        }
        inblock && (/resolve_value_op_structrefs\(/ || /resolve_immutable_structrefs\(/ || /values_equal_for_membership\(/) {
            resolved = 1
        }
        END { if (inblock) print resolved }
    ' "$EQ")

    # `found` is one or more lines (one per matched arm of that name); all must be 1.
    if [[ -z "$found" ]]; then
        echo "ERROR: handler BuiltinId::${handler} not found in builtins_equality.rs (Issue #6694)."
        errors=$((errors + 1))
    elif printf '%s\n' "$found" | grep -qx 0; then
        echo "ERROR: BuiltinId::${handler} arm does not resolve StructRefs before comparing/hashing (Issue #6694)."
        echo "       Route its operands through resolve_value_op_structrefs / resolve_immutable_structrefs."
        errors=$((errors + 1))
    fi
done

# 3. The `In` builtin (membership) must delegate to values_equal_for_membership
#    so tuple/struct elements compare by value (Issue #6691).
if [[ -f "$TYPES" ]]; then
    if ! grep -q "values_equal_for_membership(" "$TYPES"; then
        echo "ERROR: builtins_types.rs no longer routes membership through values_equal_for_membership (Issue #6691/#6694)."
        errors=$((errors + 1))
    fi
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: native value-op StructRef-resolution audit failed (Issue #6694)."
    exit 1
fi

echo "OK: every native structural value-op resolves StructRef before compare/hash (Issue #6694)."
