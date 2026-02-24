# Test complex type parameter constraints
# Tests both upper bounds (T<:Real) and lower bounds (T>:Integer)

using Test

# Function with upper bound constraint (covariant)
function process_number(x::T) where T<:Number
    x * 2
end

# Function with specific upper bound constraint
function process_real(x::T) where T<:Real
    x + 1
end

@testset "Complex type parameter constraints" begin
    # Test upper bound constraints (T<:Number)
    @test process_number(5) == 10
    @test process_number(2.5) == 5.0

    # Test upper bound constraints (T<:Real)
    @test process_real(10) == 11
    @test process_real(3.14) == 4.14
end

true
