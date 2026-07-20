# Regression fixture for Issue #8817: retire the nested-Union DeferSignature.
#
# Before Issue #8817, method signatures with `Union` types in invariant
# positions (e.g. `Vector{Union{A,B}}`, `Type{Union{A,B}}`) were deferred to
# the scoring matcher via `TypemapVerdict::DeferSignature` even though the
# subtype engine was fixed by Issue #8582 to handle them correctly.
#
# The fix introduces `core_type_is_sig_invariant_ground` — a signature-side
# predicate that accepts ground `Union`s (a Union of concrete types is a
# fully-known signature shape) while the arg-side `core_type_is_dispatch_precise`
# continues to reject `Union` (a Union-typed arg only upper-bounds the runtime
# value). The slot-support check for instantiated container parameters and
# `Type{...}` inners now uses the signature-side predicate.

# ---------------------------------------------------------------------------
# 1. Type{Union{...}} dispatch
# ---------------------------------------------------------------------------
# A method specialised on the EXACT union type object must win over a fallback.
dispatch8817_type_union(::Type{Union{Int64, Float64}}) = "type_union_int_float"
dispatch8817_type_union(::Type{Int64}) = "type_int"
dispatch8817_type_union(::Type{Float64}) = "type_float"
dispatch8817_type_union(_) = "fallback_type"

@assert dispatch8817_type_union(Union{Int64, Float64}) == "type_union_int_float"
@assert dispatch8817_type_union(Int64) == "type_int"
@assert dispatch8817_type_union(Float64) == "type_float"
@assert dispatch8817_type_union(42) == "fallback_type"

# ---------------------------------------------------------------------------
# 2. Vector{Union{...}} dispatch (Union in invariant struct parameter)
# ---------------------------------------------------------------------------
dispatch8817_vec(::Vector{Union{Int64, String}}) = "vec_union_int_str"
dispatch8817_vec(::Vector{Int64}) = "vec_int"
dispatch8817_vec(::Vector{String}) = "vec_str"
dispatch8817_vec(_) = "fallback_vec"

v_union = Vector{Union{Int64, String}}()
push!(v_union, 1)
push!(v_union, "hello")
v_int   = [1, 2, 3]
v_str   = ["a", "b"]

@assert dispatch8817_vec(v_union) == "vec_union_int_str"
@assert dispatch8817_vec(v_int) == "vec_int"
@assert dispatch8817_vec(v_str) == "vec_str"
@assert dispatch8817_vec(42) == "fallback_vec"

# ---------------------------------------------------------------------------
# 3. Type{Union{...}} with three-element union
# ---------------------------------------------------------------------------
dispatch8817_three(::Type{Union{Int64, Float64, String}}) = "three_union"
dispatch8817_three(::Type{Int64}) = "just_int"
dispatch8817_three(_) = "other"

@assert dispatch8817_three(Union{Int64, Float64, String}) == "three_union"
@assert dispatch8817_three(Int64) == "just_int"
@assert dispatch8817_three("hi") == "other"

println("All Issue #8817 nested-Union typemap dispatch tests passed")
true
