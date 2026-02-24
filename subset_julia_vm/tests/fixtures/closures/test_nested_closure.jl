# Test additional closure patterns
# Issue #1738: Additional edge case tests for closure variable capture
#
# These tests complement basic_closure.jl with additional patterns.

using Test

# Closure capturing single variable with no arguments
function make_const_fn(value)
    function const_fn()
        value
    end
    const_fn
end

# Closure with captured var used multiple times
function make_doubler(factor)
    function double_apply(x)
        factor * x + factor  # factor used twice
    end
    double_apply
end

# Closure capturing local variable (not just parameter)
function make_with_local(initial)
    computed = initial * 2
    function getter()
        computed
    end
    getter
end

# Multiple closures from same outer function (tests independence)
function make_adder(base)
    function add(x)
        base + x
    end
    add
end

@testset "Additional Closure Patterns" begin
    @testset "closure with no arguments" begin
        const5 = make_const_fn(5)
        @test const5() == 5

        const100 = make_const_fn(100)
        @test const100() == 100
    end

    @testset "captured variable used multiple times" begin
        f = make_doubler(10)
        @test f(1) == 20  # 10 * 1 + 10 = 20
        @test f(5) == 60  # 10 * 5 + 10 = 60
    end

    @testset "capture local variable" begin
        get_computed = make_with_local(5)
        @test get_computed() == 10  # 5 * 2 = 10

        get_other = make_with_local(7)
        @test get_other() == 14  # 7 * 2 = 14
    end

    @testset "multiple independent closures" begin
        add10 = make_adder(10)
        add100 = make_adder(100)

        @test add10(1) == 11
        @test add100(1) == 101
        @test add10(5) == 15
        @test add100(5) == 105
    end
end

true
