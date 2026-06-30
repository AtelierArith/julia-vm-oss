using Test

# Issue #6523: a recorded `Union{}` (Bottom) method-return snapshot must
# re-enter the inference lattice as `Bottom` (the identity of `join`), not
# `Top`. The canonical `JuliaType::Bottom` spelling of `Union{}` previously
# fell through `julia_type_to_lattice`'s `_ => Top` arm (the non-canonical
# `Union(vec![])` spelling already lowered to `Bottom`).
#
# The snapshot path needs a MULTI-method callee: a single-method call is
# inferred interprocedurally in lattice space (never converting through
# `JuliaType`), but a multi-method callee is consulted through its `MethodSig`
# `return_julia_type` snapshot (`method_return_type_to_lattice`), which is
# exactly the `JuliaType::Bottom → LatticeType` edge. `h(::Int64)`'s snapshot
# is `Union{}` (its body is an ambiguous dispatch — Issue #5603 machinery), so
# a caller branching between `h(1)` and a `Float64` literal must join to
# `Float64`, not `Any`.
#
# Upstream julia 1.12 reports exactly the same three results.

amb_6523(x::Integer, y::Real) = 1
amb_6523(x::Real, y::Integer) = "s"
h_6523(x::Int64) = amb_6523(1, 2)
h_6523(x::Float64) = 2.0
caller_6523(x::Int64) = x > 0 ? h_6523(1) : 1.5

@testset "Bottom return snapshot joins away (#6523)" begin
    @test Base.infer_return_type(amb_6523, Tuple{Int64,Int64}) === Union{}
    @test Base.infer_return_type(h_6523, Tuple{Int64}) === Union{}
    @test Base.infer_return_type(caller_6523, Tuple{Int64}) === Float64
end

true
