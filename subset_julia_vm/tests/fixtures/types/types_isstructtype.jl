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

    # Non-struct types
    @assert !isstructtype(Int64)
    @assert !isstructtype(Float64)
    @assert !isstructtype(Number)

    @test (true)
end

true  # Test passed
