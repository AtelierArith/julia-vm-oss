# Test: Basic closure capturing outer scope variables (Issue #1734)
# Verifies that nested functions can capture variables from their enclosing scope

using Test

# Basic multiplier closure - captures `factor` from outer scope
function make_multiplier(factor)
    function multiply(x)
        x * factor  # factor is captured from outer scope
    end
    multiply
end

# Closure with multiple captured variables
function make_linear(a, b)
    function linear(x)
        a * x + b  # captures both a and b
    end
    linear
end

# Closure capturing a mutable variable
function make_counter(start)
    count = start
    function increment()
        count = count + 1
        count
    end
    increment
end

@testset "Basic closure functionality" begin
    # Test make_multiplier
    mult5 = make_multiplier(5)
    @test mult5(10) == 50
    @test mult5(3) == 15
    @test mult5(0) == 0

    mult2 = make_multiplier(2)
    @test mult2(10) == 20

    # Multiple closures with different captured values
    @test mult5(7) == 35
    @test mult2(7) == 14

    # Test make_linear
    f = make_linear(2, 3)  # 2x + 3
    @test f(0) == 3
    @test f(1) == 5
    @test f(5) == 13

    g = make_linear(1, 0)  # identity: x
    @test g(42) == 42
end

true
