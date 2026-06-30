# Issue #8132: a package/user override whose return type differs from the visible
# generic. `LinearAlgebra.diag(A::StaticMatrix)` returns an `SVector`, but the
# call site only sees the generic `diag` (returns `Vector{Float64}`), so the
# binding `dg` is inferred `Vector{Float64}` while holding an `SVector` at
# runtime. Downstream typed codegen (indexing, iteration, `Tuple`, `==`) must
# fall back gracefully to the runtime value instead of crashing / mis-comparing.
# Verified line-by-line against upstream julia 1.12 + StaticArrays.
using StaticArrays
using LinearAlgebra

LinearAlgebra.diag(A::StaticMatrix) = SVector(A[1, 1], A[2, 2])
# A more-specific package method must win dispatch over the stdlib generic.
LinearAlgebra.adjoint(M::StaticMatrix) = SVector(M[1, 1], M[2, 2])

approx(a, b) = abs(a - b) < 1e-12

function iterate_sum(v)
    s = 0.0
    for x in v
        s += x
    end
    return s
end

A = @SMatrix [1.0 2.0; 3.0 4.0]
dg = diag(A)
ad = adjoint(A)

ok =
    # indexing through the inference-mismatched binding (was IndexLoadTyped crash)
    approx(dg[1], 1.0) && approx(dg[2], 4.0) &&
    # iteration / reduction over the binding (was: iterate unsupported StaticArray)
    approx(iterate_sum(dg), 5.0) && approx(sum(dg), 5.0) &&
    # Tuple construction (was compile error "Unknown function: Tuple")
    (Tuple(dg) == (1.0, 4.0)) && (Tuple([10.0, 20.0]) == (10.0, 20.0)) &&
    # equality vs a StaticArray and a native Vector (was false)
    (dg == SVector(1.0, 4.0)) && (dg == [1.0, 4.0]) &&
    ([1.0, 4.0] == SVector(1.0, 4.0)) &&
    # native-array == semantics unchanged (float identity, no isequal widening)
    ([1.0, 2.0] == [1.0, 2.0]) && !([1.0, 2.0] == [1.0, 3.0]) &&
    # the more-specific package adjoint override wins dispatch over the generic
    (ad isa SVector) && approx(ad[1], 1.0) && approx(ad[2], 4.0)

ok
