# Higher-order function edge cases test
# Tests for edge cases in callable variable dispatch (Issue #1551)
# Documents known behaviors and potential pitfalls

using Test

# Functions for testing
add(a, b) = a + b
mul(a, b) = a * b

# Multi-argument HOF variants
caller2(f, x, y) = f(x, y)
caller3(f, x, y, z) = f(x, y, z)
caller4(f, a, b, c, d) = f(a, b, c, d)

# Sum functions with different arities
sum2(a, b) = a + b
sum3(a, b, c) = a + b + c
sum4(a, b, c, d) = a + b + c + d

# Nested HOF - apply function multiple times
function apply_n_times(f, n, x, y)
    result = f(x, y)
    for _ in 2:n
        result = f(result, y)
    end
    result
end

# Single-arg functions for composition
double(x) = 2 * x
inc(x) = x + 1

# Function composition helper (single arg)
compose2(f, g, x) = f(g(x))

# Conditional function selection
get_op(use_add) = use_add ? add : mul

# Call selected function
function call_selected(use_add, x, y)
    op = get_op(use_add)
    op(x, y)
end

@testset "HOF edge cases" begin
    @testset "Multi-argument callers (2-4 args)" begin
        @test caller2(add, 1, 2) == 3
        @test caller3(sum3, 1, 2, 3) == 6
        @test caller4(sum4, 1, 2, 3, 4) == 10
    end

    @testset "Nested HOF with loop" begin
        # apply_n_times(add, 3, 2, 3) = add(add(add(2,3),3),3) = add(add(5,3),3) = add(8,3) = 11
        @test apply_n_times(add, 1, 2, 3) == 5
        @test apply_n_times(add, 2, 2, 3) == 8
        @test apply_n_times(add, 3, 2, 3) == 11

        # apply_n_times(mul, 3, 2, 3) = mul(mul(mul(2,3),3),3) = mul(mul(6,3),3) = mul(18,3) = 54
        @test apply_n_times(mul, 3, 2, 3) == 54
    end

    @testset "Function composition (3-arg style)" begin
        # compose2(double, inc, 5) = double(inc(5)) = double(6) = 12
        @test compose2(double, inc, 5) == 12

        # compose2(inc, double, 5) = inc(double(5)) = inc(10) = 11
        @test compose2(inc, double, 5) == 11
    end

    @testset "Conditional function selection" begin
        @test call_selected(true, 3, 4) == 7   # add(3, 4) = 7
        @test call_selected(false, 3, 4) == 12 # mul(3, 4) = 12
    end

    @testset "Chained HOF calls" begin
        # Pass result of one HOF to another
        @test caller2(add, caller2(mul, 2, 3), 4) == 10  # add(mul(2,3), 4) = add(6, 4) = 10
        @test caller2(mul, caller2(add, 2, 3), 4) == 20  # mul(add(2,3), 4) = mul(5, 4) = 20
    end
end

true
