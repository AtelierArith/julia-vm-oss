# Test BigInt type preservation for arithmetic operations
# Related to Issue #1688 and #1696: BigInt integer division should return BigInt, not Float64

using Test

@testset "BigInt type preservation" begin
    a = big(100)
    b = big(7)

    @testset "Division operations" begin
        # Integer division operations should preserve BigInt type
        @test typeof(a ÷ b) == BigInt
        @test typeof(div(a, b)) == BigInt
        @test (a ÷ b) == big(14)
        @test div(a, b) == big(14)

        # Remainder and modulo operations
        @test typeof(rem(a, b)) == BigInt
        @test typeof(mod(a, b)) == BigInt
        @test rem(a, b) == big(2)
        @test mod(a, b) == big(2)
    end

    @testset "Mixed type operations promote to BigInt" begin
        # Operations with BigInt and Int64 should return BigInt
        @test typeof(a ÷ 7) == BigInt
        @test typeof(div(a, 7)) == BigInt
        @test typeof(100 ÷ b) == BigInt
        @test typeof(div(100, b)) == BigInt

        # Verify the values are correct
        @test (a ÷ 7) == big(14)
        @test (100 ÷ b) == big(14)
    end

    @testset "Basic arithmetic preserves BigInt" begin
        # Addition, subtraction, multiplication should preserve BigInt
        @test typeof(a + b) == BigInt
        @test typeof(a - b) == BigInt
        @test typeof(a * b) == BigInt

        # Verify values
        @test (a + b) == big(107)
        @test (a - b) == big(93)
        @test (a * b) == big(700)
    end

    @testset "gcd and lcm preserve BigInt" begin
        # GCD and LCM should preserve BigInt type
        @test typeof(gcd(a, b)) == BigInt
        @test typeof(lcm(a, b)) == BigInt

        # Verify values
        @test gcd(a, b) == big(1)
        @test lcm(a, b) == big(700)

        # With common factors
        c = big(12)
        d = big(18)
        @test gcd(c, d) == big(6)
        @test lcm(c, d) == big(36)
    end

    @testset "Large BigInt operations" begin
        # Test with large numbers created via multiplication
        large_a = big(1000000) * big(1000000) * big(1000000)  # 10^18
        large_b = big(1000000) * big(1000000)  # 10^12

        @test typeof(large_a ÷ large_b) == BigInt
        @test typeof(div(large_a, large_b)) == BigInt
        @test (large_a ÷ large_b) == big(1000000)  # 10^6

        @test typeof(large_a + large_b) == BigInt
        @test typeof(large_a * large_b) == BigInt
    end

    # Issue #2383/#2386: Function return type preservation
    @testset "abs/abs2/sign preserve BigInt (Issue #2383)" begin
        a = big(100)
        neg_a = big(-100)

        # Unary functions should preserve BigInt type
        @test typeof(abs(a)) == BigInt
        @test typeof(abs(neg_a)) == BigInt
        @test abs(neg_a) == big(100)

        @test typeof(sign(a)) == BigInt
        @test typeof(sign(neg_a)) == BigInt
        @test sign(neg_a) == big(-1)
    end

    @testset "Variable type preservation after function calls (Issue #2383)" begin
        a = big(100)
        b = big(10)

        # Store function results in variables
        x = abs(a)
        y = gcd(a, b)

        # Variables should retain BigInt type
        @test typeof(x) == BigInt
        @test typeof(y) == BigInt

        # Operations on these variables should work correctly
        @test typeof(x ÷ y) == BigInt
        @test typeof(x * y) == BigInt
        @test (x ÷ y) == big(10)
    end

    @testset "Chained function calls preserve BigInt (Issue #2383)" begin
        a = big(100)
        b = big(10)

        # Chained calls should preserve type throughout
        @test typeof(abs(gcd(a, b))) == BigInt
        @test typeof(gcd(abs(a), abs(b))) == BigInt
        @test abs(gcd(a, b)) == big(10)
    end
end

true
