# Issue #3908 - exercise the routed NewArray / PushArrayValue / NewArrayTyped /
# LoadArray paths in subset_julia_vm_vm/src/vm/exec/array_basic.rs after their
# Value::Array constructions were centralized through push_array_ref /
# push_array_value / push_typed_array_value helpers. Behavior must remain
# identical: element type, length, shape, contents, and the kind of array that
# round-trips through a local variable should all be preserved.

# Untyped Float64 array literal: lowers to NewArray + PushElem + FinalizeArray
v_untyped = [1.0, 2.0, 3.0]
@assert v_untyped == [1.0, 2.0, 3.0]
@assert length(v_untyped) == 3
@assert eltype(v_untyped) == Float64

# Typed Int64 array literal: lowers to NewArrayTyped + PushElemTyped +
# FinalizeArrayTyped through push_typed_array_value.
v_typed = Int[10, 20, 30]
@assert v_typed == [10, 20, 30]
@assert length(v_typed) == 3
@assert eltype(v_typed) == Int64

# Typed Float64 array literal: same typed builder, different element type.
v_typed_f = Float64[1.5, 2.5, 3.5]
@assert v_typed_f == [1.5, 2.5, 3.5]
@assert eltype(v_typed_f) == Float64

# 2D untyped array literal (FinalizeArray with non-trivial shape).
m = [1 2; 3 4]
@assert size(m) == (2, 2)
@assert m[1, 1] == 1 && m[1, 2] == 2 && m[2, 1] == 3 && m[2, 2] == 4
@assert eltype(m) == Int64

# Bool typed array literal (exercises the typed builder for non-numeric storage).
b = Bool[true, false, true]
@assert b == [true, false, true]
@assert eltype(b) == Bool

# Round-trip through LoadArray: store, then read back via name lookup.
stored = [11, 22, 33]
loaded = stored
@assert loaded == [11, 22, 33]
@assert length(loaded) == 3
@assert eltype(loaded) == Int64

# Round-trip a typed Float32 array literal through LoadArray (locals_any /
# TypedArray path inside LoadArray).
f32 = Float32[1.0, 2.0]
back = f32
@assert back == Float32[1.0, 2.0]
@assert eltype(back) == Float32

# Round-trip a 2D array through LoadArray.
two_d = [1 2; 3 4]
two_d2 = two_d
@assert size(two_d2) == (2, 2)
@assert two_d2[2, 1] == 3

# Empty typed array literal (NewArrayTyped with zero capacity + FinalizeArrayTyped).
empty_typed = Int[]
@assert length(empty_typed) == 0
@assert eltype(empty_typed) == Int64

true
