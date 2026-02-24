# Test HOF parameter call with variadic splat (Issue #1657)
# Tests patterns like: function apply(f, args...); f(args...); end

using Test

# Helper functions for testing
add(x, y) = x + y
mul(x, y) = x * y
add3(x, y, z) = x + y + z
mul3(x, y, z) = x * y * z

@testset "HOF variadic splat" begin
    # Pattern: f(args...) where f is a Function parameter
    function apply_variadic(f, args...)
        return f(args...)
    end

    # Test binary function with splat
    @test apply_variadic(add, 1, 2) == 3
    @test apply_variadic(mul, 3, 4) == 12

    # Test ternary function with splat
    @test apply_variadic(add3, 1, 2, 3) == 6
    @test apply_variadic(mul3, 2, 3, 4) == 24

    # Pattern: f(x, ys...) - first arg fixed, rest splatted
    function apply_with_first(f, x, ys...)
        return f(x, ys...)
    end

    @test apply_with_first(add, 10, 5) == 15
    @test apply_with_first(mul3, 2, 3, 4) == 24

    # Pattern with typed function parameter
    function apply_typed(f::Function, args...)
        return f(args...)
    end

    @test apply_typed(add, 100, 200) == 300
end

true
