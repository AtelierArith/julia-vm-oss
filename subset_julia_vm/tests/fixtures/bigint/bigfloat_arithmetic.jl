# Test BigFloat arithmetic operations and type preservation
# Verifies that BigFloat binary operations work correctly via intrinsics
# and that mixed-type operations (BigFloat + Float64/Int64) promote to BigFloat.
# Related: Issue #2492 (BigInt dispatch regression fix), Issue #2496 (prevention)

using Test

@testset "BigFloat arithmetic and promotion" begin
    @testset "Basic BigFloat arithmetic" begin
        a = big(6.0)
        b = big(2.0)

        # Type preservation
        @test typeof(a + b) == BigFloat
        @test typeof(a - b) == BigFloat
        @test typeof(a * b) == BigFloat
        @test typeof(a / b) == BigFloat

        # Value correctness
        @test a + b == big(8.0)
        @test a - b == big(4.0)
        @test a * b == big(12.0)
        @test a / b == big(3.0)
    end

    @testset "BigFloat comparisons" begin
        a = big(3.0)
        b = big(2.0)

        @test a > b
        @test b < a
        @test a >= b
        @test b <= a
        @test a >= big(3.0)
        @test b <= big(2.0)
        @test a == big(3.0)
        @test a != b
    end

    @testset "BigFloat + Float64 mixed operations" begin
        a = big(3.14)

        # BigFloat op Float64 -> BigFloat
        @test typeof(a + 1.0) == BigFloat
        @test typeof(a - 1.0) == BigFloat
        @test typeof(a * 2.0) == BigFloat
        @test typeof(a / 2.0) == BigFloat

        # Float64 op BigFloat -> BigFloat
        @test typeof(1.0 + a) == BigFloat
        @test typeof(1.0 - a) == BigFloat
        @test typeof(2.0 * a) == BigFloat
        @test typeof(2.0 / a) == BigFloat
    end

    @testset "BigFloat + Int64 mixed operations" begin
        a = big(3.14)

        # BigFloat op Int64 -> BigFloat
        @test typeof(a + 1) == BigFloat
        @test typeof(a - 1) == BigFloat
        @test typeof(a * 2) == BigFloat

        # Int64 op BigFloat -> BigFloat
        @test typeof(1 + a) == BigFloat
        @test typeof(1 - a) == BigFloat
        @test typeof(2 * a) == BigFloat
    end

    @testset "BigFloat mixed comparisons" begin
        a = big(3.14)

        # BigFloat vs Float64
        @test a > 3.0
        @test a < 4.0

        # BigFloat vs Int64
        @test a > 3
        @test a < 4
    end
end

true
