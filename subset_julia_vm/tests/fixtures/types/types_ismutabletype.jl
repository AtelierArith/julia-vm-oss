# Test ismutabletype function

using Test

@testset "ismutabletype - check if type is mutable" begin

    # Mutable built-in types
    @assert ismutabletype(Array)
    @assert ismutabletype(Vector)
    @assert ismutabletype(Vector{Int64})
    @assert ismutabletype(Matrix)
    @assert ismutabletype(Dict)
    @assert ismutabletype(Dict{String, Int64})
    @assert ismutabletype(String)
    @assert ismutabletype(Symbol)
    @assert ismutabletype(BigInt)
    @assert ismutabletype(DataType)
    @assert ismutabletype(IOBuffer)
    @assert ismutabletype(Expr)
    @assert ismutabletype(Module)

    # Immutable or non-mutable built-in types
    @assert !ismutabletype(Int64)
    @assert !ismutabletype(Float64)
    @assert !ismutabletype(Bool)
    @assert !ismutabletype(Char)
    @assert !ismutabletype(BigFloat)
    @assert !ismutabletype(Nothing)
    @assert !ismutabletype(Missing)
    @assert !ismutabletype(Set)
    @assert !ismutabletype(Set{Int64})
    @assert !ismutabletype(Tuple)
    @assert !ismutabletype(Tuple{Int64, String})
    @assert !ismutabletype(Complex{Float64})
    @assert !ismutabletype(Rational{Int64})
    @assert !ismutabletype(Union{Int64, Float64})
    @assert !ismutabletype(QuoteNode)
    @assert !ismutabletype(LineNumberNode)
    @assert !ismutabletype(GlobalRef)

    @test true
end

true
