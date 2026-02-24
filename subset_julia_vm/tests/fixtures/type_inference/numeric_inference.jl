# Test: Numeric type inference
# The type inference engine should correctly infer numeric types
# through arithmetic operations and preserve type precision.

using Test

# Define functions outside @testset (SubsetJuliaVM limitation)
function add_ints(a, b)
    a + b
end

function multiply_floats(x, y)
    x * y
end

function mixed_arithmetic(x)
    y = x + 1      # Int64
    z = y * 2.0    # Float64 (promotion)
    z / 2          # Float64
end

function accumulate_sum(n)
    sum = 0
    for i in 1:n
        sum += i
    end
    sum
end

function calculate_average(arr)
    total = 0.0
    for x in arr
        total += x
    end
    total / length(arr)
end

function test_division()
    int_div = 7 / 2     # Float64
    int_result = div(7, 2)  # Int64
    (int_div, int_result)
end

function promote_literal_mul(x)
    y = 2.0 * x
    y
end

function promote_literal_mul_f32(x)
    y = 2.0f0 * x
    y
end

@testset "Numeric type inference" begin
    # Test integer arithmetic
    @test add_ints(5, 3) == 8
    @test add_ints(10, -5) == 5

    # Test float arithmetic
    @test multiply_floats(2.5, 4.0) == 10.0

    # Test mixed int/float arithmetic
    @test mixed_arithmetic(5) == 6.0

    # Test accumulator pattern
    @test accumulate_sum(10) == 55

    # Test type preservation in calculations
    @test calculate_average([1.0, 2.0, 3.0]) ≈ 2.0

    # Test integer division vs float division
    result = test_division()
    @test result[1] == 3.5
    @test result[2] == 3

    # Test literal promotion in arithmetic inference (e.g., 2.0 * x)
    @test promote_literal_mul(3) == 6.0
    @test typeof(promote_literal_mul(3)) == Float64
    # TODO(runtime): enable once Float32 * Float32 is supported
    # @test promote_literal_mul_f32(Float32(3.0)) == 6.0f0
    # @test typeof(promote_literal_mul_f32(Float32(3.0))) == Float32
end

true  # Test passed
