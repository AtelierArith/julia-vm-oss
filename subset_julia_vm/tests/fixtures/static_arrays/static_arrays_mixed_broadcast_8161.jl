# Issue #8161: broadcasting a StaticArray against a *dynamic* array (mixed
# static/dynamic) must broadcast element-wise, not collapse the dynamic operand
# to a scalar.
#
# Previously `SVector .- Vector` mis-lowered to `-(SVector, scalar)` (a
# `MethodError`): the static-broadcast hook only handled static-⊙-scalar
# broadcasts and deferred any container operand to the generic pipeline, which
# treats a static array as a 0-dimensional scalar. The hook now reproduces
# upstream StaticArrays' `BroadcastStyle` precedence by classifying the whole
# (fused, possibly nested) operand tree:
#   * static array(s) mixed only with scalars      -> STATIC result (SVector/SMatrix)
#   * static array mixed with any *dynamic* array  -> DYNAMIC result
#
# Upstream returns a `Sized*` array for the dynamic case (a Vector/Matrix-backed,
# statically sized container). The subset has no `Sized*` types, so it returns a
# plain `Array`; both are `AbstractArray{Float64}` with identical values, element
# type, and display, so the assertions below pass under both sjulia and upstream
# `julia` (parity-clean — they avoid the `Vector` vs `Sized*` type distinction).
using StaticArrays

a = @SVector [1.0, 2.0, 3.0]
b = [1.0, 1.0, 1.0]
m = @SMatrix [1.0 2.0; 3.0 4.0]
n = [1.0 1.0; 1.0 1.0]

# --- mixed static/dynamic -> DYNAMIC (3-element AbstractArray) result ----------
r_sub = a .- b                  # SVector .- Vector
r_rsub = b .- a                 # Vector .- SVector (static operand second)
r_absn = abs.(a .- b)           # nested fused: abs over a mixed broadcast
r_chain = a .+ b .* 2.0         # nested fused mixed, scalar inside
r_range = a .+ (1:3)            # static .⊙ a dynamic AbstractRange
r_mat = m .- n                  # SMatrix .- Matrix -> 2-D AbstractMatrix

# --- static ⊙ scalar / static ⊙ static stay STATIC (SVector) ------------------
r_scalar = a .+ 10.0            # scalar-mixed -> SVector
r_unary = a .* 2.0             # scalar-mixed -> SVector
r_ss = a .- (@SVector [1.0, 1.0, 1.0])  # SVector .- SVector -> SVector

ok = # mixed/dynamic results: right values + a non-scalar Float64 container
     r_sub == [0.0, 1.0, 2.0] && r_sub isa AbstractVector{Float64} && length(r_sub) == 3 &&
     r_rsub == [0.0, -1.0, -2.0] && r_rsub isa AbstractVector{Float64} &&
     r_absn == [0.0, 1.0, 2.0] && r_absn isa AbstractVector{Float64} &&
     r_chain == [3.0, 4.0, 5.0] && r_chain isa AbstractVector{Float64} &&
     r_range == [2.0, 4.0, 6.0] && r_range isa AbstractVector{Float64} &&
     r_mat == [0.0 1.0; 2.0 3.0] && r_mat isa AbstractMatrix{Float64} && size(r_mat) == (2, 2) &&
     # static-only broadcasts keep the static (SVector) result type
     r_scalar == [11.0, 12.0, 13.0] && r_scalar isa SVector{3,Float64} &&
     r_unary == [2.0, 4.0, 6.0] && r_unary isa SVector{3,Float64} &&
     r_ss == [0.0, 1.0, 2.0] && r_ss isa SVector{3,Float64}

println(ok)
ok
