# Test multi-line short-circuit chain (Issue #3660)
#
# Before the fix, `||` (or `&&`) followed by a newline caused
# `ParseFailed("unexpected token '\n', expected expression")` on
# the continuation line. Julia 1.12 accepts the same code: any infix
# operator at the end of a line implies the expression continues.

using Test

@testset "|| at end of line continues" begin
    function f(x)
        if (x == 1) ||
           (x == 2) ||
           (x == 3)
            return true
        end
        return false
    end

    @test f(1) == true
    @test f(2) == true
    @test f(3) == true
    @test f(4) == false
end

@testset "&& at end of line continues" begin
    function g(x, y, z)
        return x &&
               y &&
               z
    end

    @test g(true, true, true) == true
    @test g(true, false, true) == false
    @test g(false, true, true) == false
end

@testset "mixed && and || with newlines" begin
    function h(x)
        return x > 0 &&
               (x == 1 ||
                x == 5 ||
                x == 9)
    end

    @test h(1) == true
    @test h(5) == true
    @test h(9) == true
    @test h(2) == false
    @test h(-1) == false
end

@testset "+ - * with newline-after-operator" begin
    a = 1 +
        2 +
        3
    @test a == 6

    b = 10 -
        3 -
        2
    @test b == 5

    c = 2 *
        3 *
        4
    @test c == 24
end

@testset "comparison operator with newline-after" begin
    # Single comparison with newline-after-operator
    function leq(x, y)
        return x <=
               y
    end
    @test leq(1, 2) == true
    @test leq(2, 2) == true
    @test leq(3, 2) == false
end

@testset "long chain of ||" begin
    # Original textwidth-style chain (Issue #3659 motivation)
    function in_ranges(c)
        return c == 0x1100 ||
               c == 0x115F ||
               c == 0x2329 ||
               c == 0x232A ||
               c == 0x3000
    end
    @test in_ranges(0x1100) == true
    @test in_ranges(0x232A) == true
    @test in_ranges(0x3000) == true
    @test in_ranges(0x0041) == false
end

true  # Test passed
