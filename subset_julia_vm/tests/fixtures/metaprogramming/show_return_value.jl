# Test @show macro return value verification (Issue #1347)
# The @show macro should:
# 1. Print "expr = value" to stdout (side effect)
# 2. Return the evaluated value (return value)
#
# Issue #1330 revealed that only testing side effects is insufficient.
# This test explicitly verifies the return value.

using Test

# =============================================================================
# Test 1: @show with literal integer returns the integer
# =============================================================================
@testset "@show with literals" begin
    result = @show 42
    @test result == 42

    result_float = @show 3.14
    @test (result_float - 3.14) < 1e-10
end

# =============================================================================
# Test 2: @show with variable returns the variable value
# =============================================================================
@testset "@show with variables" begin
    x = 100
    result = @show x
    @test result == 100

    y = "hello"
    result_str = @show y
    @test result_str == "hello"
end

# =============================================================================
# Test 3: @show with arithmetic expression returns the computed value
# =============================================================================
@testset "@show with expressions" begin
    a = 10
    b = 20
    result = @show (a + b)
    @test result == 30

    result_mult = @show (a * b)
    @test result_mult == 200
end

# =============================================================================
# Test 4: @show with builtin function call returns the function result
# =============================================================================
@testset "@show with builtin function calls" begin
    result = @show sqrt(16.0)
    @test (result - 4.0) < 1e-10

    result_abs = @show abs(-5)
    @test result_abs == 5
end

# =============================================================================
# Test 5: @show with user-defined function returns the function result
# This was the original bug in Issue #1330 - @show f(3) returned Nothing
# =============================================================================
f(x) = 2x + 1

function g(x)
    x * x
end

@testset "@show with user-defined functions (Issue #1330)" begin
    # Short function syntax
    result_short = @show f(3)
    @test result_short == 7

    # Regular function syntax
    result_regular = @show g(4)
    @test result_regular == 16
end

# =============================================================================
# Test 6: @show with complex expressions returns correct value
# =============================================================================
@testset "@show with complex expressions" begin
    arr = [1, 2, 3, 4, 5]
    result_sum = @show sum(arr)
    @test result_sum == 15

    result_length = @show length(arr)
    @test result_length == 5
end

# =============================================================================
# Test 7: @show preserves type - return type should match expression type
# =============================================================================
@testset "@show preserves types" begin
    int_result = @show 42
    @test isa(int_result, Int64)

    float_result = @show 3.14
    @test isa(float_result, Float64)

    bool_result = @show (1 < 2)
    @test isa(bool_result, Bool)
    @test bool_result == true
end

true
