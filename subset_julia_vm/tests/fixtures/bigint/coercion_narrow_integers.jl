# Test BigInt arithmetic with narrow signed/unsigned integer operands.
# Verifies that `pop_bigint` accepts every primitive integer Value variant,
# matching official Julia's "any integer + BigInt -> BigInt" semantics.
# Related: Issue #3748 (BigInt + UInt8/16/32/64, Int8/16/32 runtime type error)

using Test

@testset "BigInt + narrow integers" begin
    a = big(100)

    @testset "Unsigned operands promote to BigInt" begin
        @test typeof(a + UInt8(7)) == BigInt
        @test typeof(a + UInt16(7)) == BigInt
        @test typeof(a + UInt32(7)) == BigInt
        @test typeof(a + UInt64(7)) == BigInt
        @test typeof(a + UInt128(7)) == BigInt

        @test a + UInt8(7) == big(107)
        @test a + UInt16(7) == big(107)
        @test a + UInt32(7) == big(107)
        @test a + UInt64(7) == big(107)
        @test a + UInt128(7) == big(107)
    end

    @testset "Narrow signed operands promote to BigInt" begin
        @test typeof(a + Int8(7)) == BigInt
        @test typeof(a + Int16(7)) == BigInt
        @test typeof(a + Int32(7)) == BigInt
        @test typeof(a + Int128(7)) == BigInt

        @test a + Int8(7) == big(107)
        @test a + Int16(7) == big(107)
        @test a + Int32(7) == big(107)
        @test a + Int128(7) == big(107)
    end

    @testset "Reverse order (narrow + BigInt)" begin
        @test typeof(UInt8(7) + a) == BigInt
        @test typeof(Int16(7) + a) == BigInt

        @test UInt8(7) + a == big(107)
        @test Int16(7) + a == big(107)
    end

    @testset "Bool operand (Bool <: Integer)" begin
        @test typeof(a + true) == BigInt
        @test a + true == big(101)
        @test a + false == big(100)
    end

    @testset "sub / mul / comparisons" begin
        @test a - UInt8(1) == big(99)
        @test a * UInt8(2) == big(200)

        @test a > UInt8(7)
        @test a == UInt8(100)
        @test UInt8(7) < a
        @test Int16(99) <= a
    end
end

true
