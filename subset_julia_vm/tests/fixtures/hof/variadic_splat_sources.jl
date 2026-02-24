# Test HOF variadic splat with different collection sources (Issue #1675)
# Edge cases: array splat, tuple splat, range splat, nested HOF with splat
# Verifies CallFunctionVariableWithSplat handles all splat expansion types

using Test

# Helper functions
add(x, y) = x + y
add3(x, y, z) = x + y + z
mul(x, y) = x * y

# HOF that splats an array argument
function apply_array(f, arr)
    return f(arr...)
end

# HOF that splats a tuple argument
function apply_tuple(f, t)
    return f(t...)
end

# HOF that splats a range argument
function apply_range(f, r)
    return f(r...)
end

# Nested HOF: apply g to splatted args, then apply f to the result
function nested_apply(f, g, args...)
    return f(g(args...))
end

# Pre-compute results outside @testset (Issue #1722 workaround)
result_array_add = apply_array(add, [10, 20])
result_array_add3 = apply_array(add3, [1, 2, 3])
result_tuple_add = apply_tuple(add, (5, 7))
result_tuple_add3 = apply_tuple(add3, (10, 20, 30))
result_range_add3 = apply_range(add3, 1:3)
result_nested = nested_apply(x -> x * 2, add, 3, 4)
result_nested_add3 = nested_apply(x -> x + 100, add3, 1, 2, 3)

@testset "HOF variadic splat sources" begin
    # Array splat: f([1,2,3]...) expands array elements as arguments
    @test result_array_add == 30
    @test result_array_add3 == 6

    # Tuple splat: f((1,2)...) expands tuple elements as arguments
    @test result_tuple_add == 12
    @test result_tuple_add3 == 60

    # Range splat: f(1:3...) expands range into 1, 2, 3
    @test result_range_add3 == 6

    # Nested HOF: nested_apply(double, add, 3, 4) = double(add(3,4)) = double(7) = 14
    @test result_nested == 14
    # nested_apply(x -> x + 100, add3, 1, 2, 3) = (1+2+3) + 100 = 106
    @test result_nested_add3 == 106
end

true
