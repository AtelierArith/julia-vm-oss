using Test

# Issue #8893: UInt64 typemax mixed with Int128 exits during conversion.
# typemax(UInt64) + Int128(1) should work without crashing.

let
    x = typemax(UInt64)
    @test typeof(x) == UInt64
    @test x == 18446744073709551615

    y = Int128(1)
    @test typeof(x + y) == Int128
    @test x + y == Int128(18446744073709551616)

    @test typeof(x - y) == Int128
    @test x - y == Int128(18446744073709551614)

    @test typeof(x * y) == Int128
    @test x * y == Int128(18446744073709551615)

    # Mixed UInt64 + Int128 arithmetic
    a = typemax(UInt64)
    b = Int128(2)
    @test typeof(a + b) == Int128
    @test a + b == Int128(18446744073709551617)
end

true
