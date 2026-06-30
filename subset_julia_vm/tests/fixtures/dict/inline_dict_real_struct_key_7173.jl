# Issue #7173: indexing a freshly-constructed `Dict(...)` INLINE (without binding
# it to a variable first) with a key whose type is a user struct `<: Real`
# routed `getindex` to the numeric array-index path and failed with
#   Type error: expected I64 or CartesianIndex, got <StructType>
#
# Root cause: `is_dict_struct_name` stripped a module prefix with `rsplit('.')`
# over the WHOLE parametric name, so a key type carrying a dot inside its type
# parameters (e.g. `Dict{M.R, Int64}`, or `Dict{Symbolics.Num, Int64}`) was
# mis-split to `R, Int64}` and failed the `== "Dict"` check. The inline
# `Dict(...)` receiver was then not recognized as a Dict at compile time, and a
# `<: Real` key made the fallback choose numeric indexing.
#
# A module-qualified key name (here `M.R`) is needed to trigger the dot-inside-
# params path; a bare top-level `R` has no dot and was never affected.
# All forms below match upstream Julia 1.12.6.

module M
    struct R <: Real
        v::Int
    end
    Base.:(==)(a::R, b::R) = a.v == b.v
    Base.hash(a::R, h::UInt) = hash(a.v, h)
end
using .M: R

checks = Bool[]

# Bound dict (always worked)
d = Dict(R(1) => 10)
push!(checks, d[R(1)] == 10)

# Inline-chained construction + index (the bug)
push!(checks, Dict(R(1) => 10)[R(1)] == 20 - 10)
push!(checks, Dict(R(2) => 7, R(3) => 8)[R(3)] == 8)

# A second `<: Real` key type to exercise more than one struct
module N
    struct S <: Real
        w::Int
    end
    Base.:(==)(a::S, b::S) = a.w == b.w
    Base.hash(a::S, h::UInt) = hash(a.w, h)
end
using .N: S
push!(checks, Dict(S(5) => 50)[S(5)] == 50)

# Regression: ordinary numeric array indexing must be unaffected.
v = [10, 20, 30]
push!(checks, v[2] == 20)
A = [1 2; 3 4]
push!(checks, A[2, 1] == 3)
push!(checks, [100, 200, 300][3] == 300)   # inline array index still numeric

# Regression: an inline plain Dict with an Int key still works inline.
push!(checks, Dict(1 => 11, 2 => 22)[2] == 22)
push!(checks, Dict("a" => 1)["a"] == 1)

all(checks)
