# Test function dispatch inside parametric functions (where T context)
# Related to Issue #2384: Parametric type dispatch limitation
#
# Known limitation: Inside `where T` functions, dispatch does NOT resolve
# type parameters at compile time. This test documents the workarounds.

using Test

# Helper that uses intrinsic directly (the workaround pattern)
function _safe_div(x::Int64, y::Int64)
    return sdiv_int(x, y)
end

@testset "Parametric function dispatch (Issue #2388)" begin
    @testset "Workaround: intrinsic helper preserves Int64" begin
        # The workaround pattern: use intrinsic directly
        function safe_reduce(x::T, y::T) where T
            g = gcd(x, y)
            if g > 1
                return _safe_div(Int64(x), Int64(g))
            end
            return x
        end

        result = safe_reduce(Int64(6), Int64(4))
        @test result == 3
        @test typeof(result) == Int64
    end

    @testset "gcd preserves type in where T context" begin
        # gcd has return type override in call.rs
        function test_gcd(x::T, y::T) where T
            return gcd(x, y)
        end

        result = test_gcd(Int64(12), Int64(8))
        @test result == 4
        @test typeof(result) == Int64
    end

    @testset "abs preserves type in where T context" begin
        # abs has return type override in call.rs
        function test_abs(x::T) where T
            return abs(x)
        end

        result = test_abs(Int64(-5))
        @test result == 5
        @test typeof(result) == Int64
    end

    @testset "Rational constructor preserves Int64 (Issue #2384 fix)" begin
        # The specific case that triggered the original bug
        r = 3//6
        @test r.num == 1
        @test r.den == 2

        # More reduction cases
        r2 = 6//8
        @test r2.num == 3
        @test r2.den == 4

        r3 = 100//25
        @test r3.num == 4
        @test r3.den == 1
    end
end

true
