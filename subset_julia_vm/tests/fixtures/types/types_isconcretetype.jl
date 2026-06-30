# Test isconcretetype function

using Test

@testset "isconcretetype - check if type is concrete" begin

    # Concrete types (can have instances)
    @assert isconcretetype(Int64)
    @assert isconcretetype(Float64)
    @assert isconcretetype(Bool)
    @assert isconcretetype(Char)
    @assert isconcretetype(String)
    @assert isconcretetype(Nothing)
    @assert isconcretetype(Missing)
    @assert isconcretetype(BigInt)
    @assert isconcretetype(BigFloat)
    @assert isconcretetype(Symbol)
    @assert isconcretetype(DataType)
    @assert isconcretetype(Complex{Float64})
    @assert isconcretetype(Rational{Int64})
    @assert isconcretetype(Vector{Int64})
    @assert isconcretetype(Dict{String, Int64})
    @assert isconcretetype(Set{Int64})
    @assert isconcretetype(UnitRange{Int64})
    @assert isconcretetype(Expr)
    @assert isconcretetype(QuoteNode)
    @assert isconcretetype(LineNumberNode)
    @assert isconcretetype(GlobalRef)
    @assert isconcretetype(Module)

    # Abstract types (cannot have instances directly)
    @assert !isconcretetype(Integer)
    @assert !isconcretetype(Real)
    @assert !isconcretetype(Number)
    @assert !isconcretetype(Any)
    @assert !isconcretetype(AbstractFloat)
    @assert !isconcretetype(Type)
    @assert !isconcretetype(Type{Int64})
    @assert !isconcretetype(Tuple)
    @assert !isconcretetype(Vector)
    @assert !isconcretetype(Union{Int64, Float64})

    @test (true)
end

true  # Test passed
