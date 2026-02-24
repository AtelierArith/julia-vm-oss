# Test isprimitivetype function

using Test

@testset "isprimitivetype - check if type is primitive" begin

    # Primitive types (fixed bit width, no fields)
    @assert isprimitivetype(Bool)
    @assert isprimitivetype(Int64)
    @assert isprimitivetype(Float64)
    @assert isprimitivetype(Char)

    # Non-primitive types
    @assert !isprimitivetype(String)
    @assert !isprimitivetype(Number)
    @assert !isprimitivetype(Any)

    @test (true)
end

true  # Test passed
