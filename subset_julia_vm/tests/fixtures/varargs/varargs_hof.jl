# Test Higher-Order Functions with varargs parameters (Issue #1679)
# HOF patterns like apply(f, args...) should work correctly
# This tests the fix for Issue #1657 (HOF parameter call with variadic splat)
# Note: Issues #1721 (short-form varargs) and #1722 (closure over varargs) are now fixed
# and have dedicated regression tests in varargs_short_form.jl and varargs_closure.jl

using Test

# Basic HOF that passes varargs to another function
function apply_all(f, args...)
    f(args...)
end

# Simple target functions (using full form for varargs)
function add2(a, b)
    a + b
end

function add3(a, b, c)
    a + b + c
end

function sum_all(args...)
    sum(args)
end

function count_args(args...)
    length(args)
end

function double(x)
    x * 2
end

# Nested HOF with varargs
function compose_apply(f, g, args...)
    result = g(args...)
    f(result)
end

# Multiple function parameters with varargs
function apply_two(f, g, args...)
    f_result = f(args...)
    g_result = g(args...)
    (f_result, g_result)
end

# Pre-compute results outside testset
result_apply_add2 = apply_all(add2, 3, 4)
result_apply_add3 = apply_all(add3, 1, 2, 3)
result_apply_sum = apply_all(sum_all, 1, 2, 3, 4, 5)
result_apply_count_empty = apply_all(count_args)
result_compose = compose_apply(double, sum_all, 1, 2, 3)
result_two = apply_two(sum_all, count_args, 1, 2, 3)

@testset "Higher-Order Functions with varargs" begin
    # Basic apply pattern
    @test result_apply_add2 == 7
    @test result_apply_add3 == 6
    @test result_apply_sum == 15

    # Empty varargs - count_args() returns 0
    @test result_apply_count_empty == 0

    # Nested HOF - compose_apply(double, sum_all, 1, 2, 3) = double(sum(1,2,3)) = double(6) = 12
    @test result_compose == 12

    # Multiple HOF applications
    # apply_two(sum_all, count_args, 1, 2, 3) = (sum(1,2,3), count_args(1,2,3)) = (6, 3)
    @test result_two[1] == 6
    @test result_two[2] == 3
end

true
