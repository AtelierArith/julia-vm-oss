# Test supertype function

using Test

@testset "supertype - get parent type in type hierarchy" begin

    # Test that supertype returns correct types by checking type names
    @assert string(supertype(Int64)) == "Signed"
    @assert string(supertype(UInt64)) == "Unsigned"
    @assert string(supertype(Float64)) == "AbstractFloat"
    @assert string(supertype(Bool)) == "Integer"
    @assert string(supertype(Char)) == "AbstractChar"
    @assert string(supertype(String)) == "AbstractString"

    # Abstract types
    @assert string(supertype(Signed)) == "Integer"
    @assert string(supertype(Integer)) == "Real"
    @assert string(supertype(Real)) == "Number"
    @assert string(supertype(Number)) == "Any"
    @assert string(supertype(AbstractFloat)) == "Real"

    # Rank/dim-generic type aliases (`Vector{T} = Array{T,1}`,
    # `AbstractMatrix{T} = AbstractArray{T,2}`, etc.) are UnionAll aliases
    # upstream; `supertype` on a bare UnionAll recurses through the body
    # instead of collapsing to the rank-ERASED family (Issues #10282, #10283).
    @assert string(supertype(AbstractMatrix)) == "Any"
    @assert string(supertype(AbstractVector)) == "Any"
    # PARAMETRIC instantiations of the abstract rank-generic aliases collapse
    # to `Any` too — `AbstractArray{T,N}`'s declared parent does not depend on
    # the element type — never the rank-erased `AbstractArray` family
    # (Issue #10314).
    @assert string(supertype(AbstractVector{Int64})) == "Any"
    @assert string(supertype(AbstractMatrix{Int64})) == "Any"
    @assert string(supertype(DenseVector)) == "AbstractVector"
    @assert string(supertype(DenseMatrix)) == "AbstractMatrix"
    @assert string(supertype(AbstractRange)) == "AbstractVector"
    @assert string(supertype(OrdinalRange)) == "AbstractRange"
    @assert startswith(string(supertype(AbstractUnitRange)), "OrdinalRange")

    # The BitArray family are zero-free-typevar concrete aliases
    # (`BitVector = BitArray{1}`, `BitArray{N} <: AbstractArray{Bool,N}`):
    # unlike the rank-generic `Vector{T}` aliases, the element type is FIXED
    # to `Bool`, so `supertype` always carries the `Bool` parameter
    # (Issue #10312).
    @assert string(supertype(BitVector)) == "AbstractVector{Bool}"
    @assert string(supertype(BitMatrix)) == "AbstractMatrix{Bool}"
    @assert string(supertype(BitArray)) == "AbstractArray{Bool}"
    @assert string(supertype(BitArray{1})) == "AbstractVector{Bool}"
    @assert string(supertype(BitArray{2})) == "AbstractMatrix{Bool}"
    @assert string(supertype(BitArray{3})) == "AbstractArray{Bool, 3}"

    # Builtin concrete families
    @assert string(supertype(Vector)) == "DenseVector"
    @assert string(supertype(Matrix)) == "DenseMatrix"
    @assert string(supertype(Dict)) == "AbstractDict"
    @assert startswith(string(supertype(UnitRange)), "AbstractUnitRange")
    @assert string(supertype(StepRange)) == "OrdinalRange"
    @assert string(supertype(IOBuffer)) == "IO"
    @assert startswith(string(supertype(DataType)), "Type")

    # Any is its own supertype
    @assert string(supertype(Any)) == "Any"

    @test (true)
end

true  # Test passed
