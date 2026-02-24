# Test basic UnionAll type functionality

using Test

# Function with type parameter
function identity_typed(x::T) where T
    x
end

# Function with bounded type parameter
function double_number(x::T) where T<:Number
    x + x
end

@testset "UnionAll basic functionality" begin
    # Test identity function with numeric types
    @test identity_typed(42) == 42
    @test identity_typed(3.14) == 3.14

    # Test bounded type parameter
    @test double_number(5) == 10
    @test double_number(2.5) == 5.0
end

true
