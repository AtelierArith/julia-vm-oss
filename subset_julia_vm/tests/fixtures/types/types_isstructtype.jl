# Test isstructtype function

using Test

struct Point
    x::Float64
    y::Float64
end

@testset "isstructtype - check if type is a struct" begin


    # User-defined structs
    @assert isstructtype(Point)

    # Built-in struct-like types
    @assert isstructtype(String)
    @assert isstructtype(Symbol)
    @assert isstructtype(BigInt)
    @assert isstructtype(BigFloat)
    @assert isstructtype(Nothing)
    @assert isstructtype(Missing)
    @assert isstructtype(DataType)
    @assert isstructtype(Tuple)
    @assert isstructtype(Tuple{Int64, String})
    @assert isstructtype(Vector)
    @assert isstructtype(Vector{Int64})
    @assert isstructtype(Dict{String, Int64})
    @assert isstructtype(Set{Int64})
    @assert isstructtype(Complex{Float64})
    @assert isstructtype(Rational{Int64})
    @assert isstructtype(Expr)
    @assert isstructtype(QuoteNode)
    @assert isstructtype(LineNumberNode)
    @assert isstructtype(GlobalRef)
    @assert isstructtype(Module)

    # Non-struct types
    @assert !isstructtype(Int64)
    @assert !isstructtype(Float64)
    @assert !isstructtype(Bool)
    @assert !isstructtype(Char)
    @assert !isstructtype(Number)
    @assert !isstructtype(Function)
    @assert !isstructtype(IO)
    @assert !isstructtype(Type)
    @assert !isstructtype(Type{Int64})
    @assert !isstructtype(Union{Int64, Float64})

    @test (true)
end

true  # Test passed
