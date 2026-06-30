# Issue #3908 - exercise the routed PushElem / FinalizeArray / PushElemTyped /
# FinalizeArrayTyped / LoadArray / StoreArray paths in
# subset_julia_vm/src/vm/exec/array_basic.rs after their native-Array
# destructure sites were centralized through the file-local
# `legacy_array_value_mut_ref` (PushElem / FinalizeArray / PushElemTyped /
# FinalizeArrayTyped), `legacy_array_value_into` (LoadArray's four owned-value
# sites), and `try_consume_array_value` (StoreArray) helpers. Behavior must
# remain identical for typed and untyped literals, 2D shapes, LoadArray
# round-trips through both the current and the global frame, and StoreArray
# of the Set / Dict / Array runtime-fallback variants.

# Untyped Float64 literal (PushElem + FinalizeArray under the mut helper).
vu = [3.5, 2.5, 1.5]
@assert vu == [3.5, 2.5, 1.5]
@assert length(vu) == 3
@assert eltype(vu) == Float64

# Typed Int literal (PushElemTyped + FinalizeArrayTyped under the mut helper).
vi = Int[7, 8, 9]
@assert vi == [7, 8, 9]
@assert length(vi) == 3
@assert eltype(vi) == Int64

# Typed Float32 literal exercising the typed PushElemTyped path with a
# different element width.
vf32 = Float32[1.5, 2.5]
@assert length(vf32) == 2
@assert vf32[1] == Float32(1.5)
@assert vf32[2] == Float32(2.5)
@assert eltype(vf32) == Float32

# 2D untyped literal: FinalizeArray with non-trivial shape under the mut helper.
m = [10 20; 30 40]
@assert size(m) == (2, 2)
@assert m[1, 1] == 10 && m[2, 2] == 40
@assert eltype(m) == Int64

# LoadArray round-trip through the current frame's slot/local lookup path
# (legacy_array_value_into chained off load_slot_value_by_name and locals_any).
loaded = vi
@assert loaded == [7, 8, 9]
@assert length(loaded) == 3

# StoreArray of an Array variant (try_consume_array_value Ok(arr) arm).
saved = [100, 200, 300]
@assert saved == [100, 200, 300]
@assert eltype(saved) == Int64

# StoreArray of a Set variant (try_consume_array_value Err(Set) arm: stored in
# locals_any with VarTypeTag::Any).
s = Set([1, 2, 3])
@assert length(s) == 3
@assert 2 in s

# StoreArray of a Dict variant (try_consume_array_value Err(Dict) arm: stored
# in locals_any with VarTypeTag::Any).
d = Dict("a" => 1, "b" => 2)
@assert d["a"] == 1
@assert d["b"] == 2

# LoadArray fallback through the global frame: a function reading a global
# Array (locals_array.get(name) re-push under push_array_ref).
gvec = Int[42, 43, 44]
function read_global()
    return gvec
end
gout = read_global()
@assert gout == [42, 43, 44]
@assert eltype(gout) == Int64
@assert length(gout) == 3

# LoadArray fallback through the global frame for a typed Float32 array
# (locals_any.get(name) path inside the global-frame fallback).
gf32 = Float32[9.5, 8.5]
function read_global_f32()
    return gf32
end
gf32_out = read_global_f32()
@assert length(gf32_out) == 2
@assert gf32_out[1] == Float32(9.5)
@assert gf32_out[2] == Float32(8.5)
@assert eltype(gf32_out) == Float32

true
