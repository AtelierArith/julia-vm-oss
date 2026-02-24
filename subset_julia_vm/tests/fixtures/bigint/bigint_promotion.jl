# Test BigInt promotion and mixed-type operations
# Verifies that BigInt correctly participates in the type hierarchy
# (BigInt <: Signed <: Integer <: Real <: Number) and that mixed-type
# operations with Int64/Int128 work via the intrinsic dispatch guard.
# Related: Issue #2492 (BigInt dispatch regression fix), Issue #2496 (prevention)

using Test

@testset "BigInt promotion and mixed-type operations" begin
    @testset "BigInt + BigInt basic" begin
        a = big(100)
        b = big(7)

        @test typeof(a + b) == BigInt
        @test typeof(a - b) == BigInt
        @test typeof(a * b) == BigInt

        @test a + b == big(107)
        @test a - b == big(93)
        @test a * b == big(700)
    end

    @testset "BigInt + Int64 mixed" begin
        a = big(100)

        # BigInt op Int64 -> BigInt
        @test typeof(a + 7) == BigInt
        @test typeof(a - 7) == BigInt
        @test typeof(a * 7) == BigInt

        # Int64 op BigInt -> BigInt
        @test typeof(100 + big(7)) == BigInt
        @test typeof(100 - big(7)) == BigInt
        @test typeof(100 * big(7)) == BigInt

        # Value correctness
        @test a + 7 == big(107)
        @test 100 + big(7) == big(107)
    end

    @testset "BigInt comparisons with Int64" begin
        a = big(100)

        @test a > 7
        @test a >= 100
        @test a == 100
        @test a != 99
        @test a < 200
        @test a <= 100

        @test 200 > a
        @test 100 >= a
        @test 100 == a
    end

    @testset "BigInt subtype checks" begin
        # BigInt should be recognized as a subtype of numeric abstract types
        # This tests the check_subtype() fix from Issue #2492
        @test big(42) isa Integer
        @test big(42) isa Real
        @test big(42) isa Number
    end
end

true
