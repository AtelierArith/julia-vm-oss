#!/usr/bin/env bash
# Issue #6694 (prevention for the StructRef value-op family #6685 tuple-`==` /
# #6691 `in` / #6693 hash / #6709 `===`), TYPE-ANCHORED per Issue #8919.
#
# Immutable structs (OneTo, UnitRange, ordinary non-`mutable` user structs) are
# stored on the struct heap as `Value::StructRef(idx)`. Any native VM op that
# compares or hashes a `Value` by STRUCTURE — `==`/`isequal` over tuples/
# named-tuples/structs, `hash`/`_hash`, `in`/`∈` membership, `isless`, and `===`
# over immutable structs — must first resolve those heap refs to inline
# snapshots, otherwise it compares/hashes heap indices and two separately-
# constructed but equal values are reported unequal (or hash differently).
#
# HOW THIS IS ENFORCED NOW (Issue #8919): the invariant is a TYPE, not a grep.
# The former version of this audit scanned each `BuiltinId::{Egal,TupleEquals,
# Hash,_Hash}` handler arm with awk for a resolver call (a fragile,
# arm-boundary/naming-dependent heuristic — the exact class of grep audit #8642
# retired for the display sinks). The structural compare/hash core sinks in
# `vm/builtins_equality.rs` now take the `StructResolved` witness newtype, whose
# only constructors resolve. Feeding an unresolved `Value` to a sink is therefore
# a COMPILE error — `cargo build` is the real gate.
#
# This script is the belt-and-suspenders that keeps the TYPE WALL standing: it
# fails if the witness type, one of its resolving constructors, or a sink's
# `&StructResolved` requirement is removed (any of which would let a future arm
# fold an unresolved operand again), and if membership stops routing through the
# resolving wrapper. It checks the type contract, not per-arm resolver calls.
#
# Adding a NEW native structural compare/hash op? Make its core sink take
# `&StructResolved` and add the sink's function name to CORE_SINKS below.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EQ="$ROOT/subset_julia_vm_vm/src/vm/builtins_equality.rs"
TYPES="$ROOT/subset_julia_vm_vm/src/vm/builtins_types.rs"

errors=0

if [[ ! -f "$EQ" ]]; then
    echo "ERROR: $EQ missing — update check_native_value_ops_resolve_structref.sh (Issue #6694/#8919)."
    exit 1
fi

# 1. The `StructResolved` witness type must still be defined. Without it the
#    whole type wall is gone and the sinks below would take raw `&Value`.
if ! grep -q "struct StructResolved<'a>" "$EQ"; then
    echo "ERROR: the StructResolved witness type is gone from builtins_equality.rs (Issue #8919)."
    echo "       It is the type wall replacing the retired per-arm resolver grep — do not remove it."
    errors=$((errors + 1))
fi

# 2. The resolving constructors must exist. These are the ONLY ways to obtain a
#    `StructResolved`, and each performs the heap resolution. If one is renamed
#    or dropped, update this list — but a sink must never accept a value that
#    skipped resolution.
for ctor in \
    "fn all_structs(" \
    "fn all_structs_ref(" \
    "fn egal_resolved(" ; do
    if ! grep -q "$ctor" "$EQ"; then
        echo "ERROR: StructResolved resolving constructor '$ctor' is gone from builtins_equality.rs (Issue #8919)."
        echo "       A witness must only be obtainable through a resolving constructor."
        errors=$((errors + 1))
    fi
done

# 3. Every structural compare/hash CORE SINK must REQUIRE the `StructResolved`
#    witness in its signature. This is the compile-error wall: a sink that takes
#    raw `&Value` again could fold an unresolved heap index. The check scans the
#    function's signature block (from `fn NAME(` up to the line that closes the
#    parameter list with `) ->` or `) {`) for `StructResolved`.
CORE_SINKS="egal_compare_witnessed isequal_compare_witnessed tuple_equals_witnessed values_equal_witnessed isless_compare_witnessed hash_resolved_value"

for sink in $CORE_SINKS; do
    if ! grep -q "fn ${sink}(" "$EQ"; then
        echo "ERROR: structural compare/hash core sink '${sink}' not found in builtins_equality.rs (Issue #8919)."
        echo "       Add it back or update CORE_SINKS if it was renamed."
        errors=$((errors + 1))
        continue
    fi
    # Signature block: the `fn NAME(` line and following lines up to the first
    # line that closes the parameter list with `) ->` or `) {`.
    witnessed=$(awk -v target="fn ${sink}(" '
        index($0, target) { insig = 1 }
        insig {
            if ($0 ~ /StructResolved/) { found = 1 }
            if ($0 ~ /\) *(->|\{)/) { print (found ? 1 : 0); exit }
        }
    ' "$EQ")
    if [[ "$witnessed" != "1" ]]; then
        echo "ERROR: core sink '${sink}' does not require the StructResolved witness (Issue #8919)."
        echo "       Its signature must take &StructResolved / StructResolved so an unresolved"
        echo "       operand is a compile error — do not downgrade it to raw &Value."
        errors=$((errors + 1))
    fi
done

# 4. The `In` builtin (membership) must delegate to the resolving wrapper
#    `values_equal_for_membership`, which builds `StructResolved` witnesses
#    (Issue #6691). Membership operands live in builtins_types.rs.
if [[ -f "$TYPES" ]]; then
    if ! grep -q "values_equal_for_membership(" "$TYPES"; then
        echo "ERROR: builtins_types.rs no longer routes membership through values_equal_for_membership (Issue #6691/#6694)."
        echo "       Tuple/struct membership must resolve StructRefs via the witness wrapper."
        errors=$((errors + 1))
    fi
fi

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: native value-op StructRef-resolution type wall audit failed (Issue #6694/#8919)."
    exit 1
fi

echo "OK: the StructResolved witness type wall stands — every structural compare/hash core sink"
echo "    requires resolution before the structural fold (Issue #6694/#8919)."
