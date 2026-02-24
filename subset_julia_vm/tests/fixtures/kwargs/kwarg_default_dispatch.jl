# Test: Keyword argument function dispatch with defaults (Issue #1331)
# Verifies that functions with keyword arguments can be called without
# explicitly providing kwargs - the defaults should be used.

using Test

# Simple kwarg function (short form) - exact case from Issue #1331
f(x; y=10) = x + y

# Long form function with kwarg
function g(x; y=20)
    return x + y
end

# Multiple positional args with kwarg
h(a, b; c=100) = a + b + c

@testset "Keyword argument dispatch with defaults (Issue #1331)" begin
    # Test calling with only positional argument - kwarg should use default
    @test f(1) == 11      # 1 + 10 (default)
    @test f(5) == 15      # 5 + 10 (default)

    # Test calling with explicit kwarg
    @test f(1, y=20) == 21
    @test f(1; y=20) == 21

    # Test long form function
    @test g(5) == 25      # 5 + 20 (default)
    @test g(5, y=30) == 35

    # Test function with multiple positional args
    @test h(1, 2) == 103  # 1 + 2 + 100 (default)
    @test h(1, 2, c=50) == 53
end

true
