# Test @time macro return value verification
# The @time macro should:
# 1. Print elapsed time to stdout (side effect)
# 2. Return the evaluated expression result (return value)
#
# Following Issue #1347's guidance on complete macro testing.

using Test

# =============================================================================
# Test 1: @time with literal returns the literal
# =============================================================================
@testset "@time with literals" begin
    result = @time 42
    @test result == 42

    result_float = @time 3.14
    @test (result_float - 3.14) < 1e-10
end

# =============================================================================
# Test 2: @time with variable returns the variable value
# =============================================================================
@testset "@time with variables" begin
    x = 100
    result = @time x
    @test result == 100
end

# =============================================================================
# Test 3: @time with arithmetic expression returns the computed value
# =============================================================================
@testset "@time with expressions" begin
    result = @time (10 + 20)
    @test result == 30

    a = 5
    b = 7
    result_mult = @time (a * b)
    @test result_mult == 35
end

# =============================================================================
# Test 4: @time with function call returns the function result
# =============================================================================
@testset "@time with function calls" begin
    result = @time sqrt(25.0)
    @test (result - 5.0) < 1e-10
end

# =============================================================================
# Test 5: @time with user-defined function returns correct value
# =============================================================================
compute(x) = x * x + 1

@testset "@time with user-defined functions" begin
    result = @time compute(5)
    @test result == 26
end

# =============================================================================
# Test 6: @time with block expression returns the last value
# =============================================================================
@testset "@time with block expressions" begin
    result = @time begin
        a = 10
        b = 20
        a + b
    end
    @test result == 30
end

# =============================================================================
# Test 7: @time preserves types
# =============================================================================
@testset "@time preserves types" begin
    int_result = @time 42
    @test isa(int_result, Int64)

    float_result = @time 3.14
    @test isa(float_result, Float64)

    bool_result = @time (2 > 1)
    @test isa(bool_result, Bool)
    @test bool_result == true
end

true
