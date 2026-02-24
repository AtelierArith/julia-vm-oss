# Test BigInt power operations (Issue #1708)
# BigInt power with Int64 exponent

using Test

@testset "BigInt power operations" begin
    @testset "Basic power operations" begin
        # big(n)^e should return BigInt
        @test big(2)^10 == big(1024)
        @test big(10)^5 == big(100000)
        @test big(3)^4 == big(81)

        # Verify type preservation
        @test typeof(big(2)^10) == BigInt
        @test typeof(big(10)^5) == BigInt
    end

    @testset "Edge cases" begin
        # Power of 0
        @test big(5)^0 == big(1)
        @test big(100)^0 == big(1)

        # Power of 1
        @test big(5)^1 == big(5)
        @test big(100)^1 == big(100)

        # Base of 0
        @test big(0)^5 == big(0)

        # Base of 1
        @test big(1)^100 == big(1)
    end

    @testset "Large exponents" begin
        # These would overflow Int64 but work with BigInt
        result_2_64 = big(2)^64
        @test typeof(result_2_64) == BigInt

        result_10_20 = big(10)^20
        @test typeof(result_10_20) == BigInt

        # Test large exponent values (verify computation works)
        @test big(2)^32 == big(4294967296)
        @test big(10)^10 == big(10000000000)
    end

    @testset "Negative base" begin
        # Negative base with even exponent
        @test big(-2)^4 == big(16)
        @test big(-3)^2 == big(9)

        # Negative base with odd exponent
        @test big(-2)^3 == big(-8)
        @test big(-3)^3 == big(-27)
    end
end

true
