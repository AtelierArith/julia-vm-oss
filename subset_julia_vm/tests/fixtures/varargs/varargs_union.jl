# Test varargs parameters with Union type annotations (Issue #1685)
# Typed varargs like (xs::Union{Int64,Float64}...) should work correctly

using Test

# Helper functions defined outside testset
# Union type varargs - accepts Int64 or Float64 arguments
function sum_numbers(xs::Union{Int64,Float64}...)
    total = 0.0
    for x in xs
        total += x
    end
    total
end

function count_numbers(xs::Union{Int64,Float64}...)
    length(xs)
end

function mul_numbers(xs::Union{Int64,Float64}...)
    result = 1.0
    for x in xs
        result *= x
    end
    result
end

@testset "Union type varargs parameters" begin
    # Test with no arguments
    @test count_numbers() == 0

    # Test with Int64 arguments only
    @test count_numbers(1, 2, 3) == 3
    @test sum_numbers(1, 2, 3) == 6.0

    # Test with Float64 arguments only
    @test count_numbers(1.0, 2.0, 3.0) == 3
    @test sum_numbers(1.0, 2.0, 3.0) == 6.0

    # Test with mixed Int64 and Float64 arguments
    @test count_numbers(1, 2.0, 3) == 3
    @test sum_numbers(1, 2.5, 3) == 6.5

    # Test multiplication
    @test mul_numbers(2, 3, 4) == 24.0
    @test mul_numbers(2.0, 3.0, 4.0) == 24.0
    @test mul_numbers(2, 3.0, 4) == 24.0
end

true
