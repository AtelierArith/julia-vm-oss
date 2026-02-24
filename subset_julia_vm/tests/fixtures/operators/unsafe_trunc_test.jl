# Test unsafe_trunc function: Unsafe truncation without error checking

using Test

@testset "unsafe_trunc function: Unsafe truncation without error checking" begin

    # === Basic truncation (Int64) ===
    @test unsafe_trunc(Int64, 3.7) == 3
    @test unsafe_trunc(Int64, -2.2) == -2
    @test unsafe_trunc(Int64, 5.0) == 5
    @test unsafe_trunc(Int64, -5.0) == -5

    # === Different integer types ===
    # Note: Convert results to Int64 for comparison since Int32/Int16/Int8 == Int64 is not supported
    @test Int64(unsafe_trunc(Int32, 3.7)) == 3
    @test Int64(unsafe_trunc(Int32, -2.2)) == -2
    @test Int64(unsafe_trunc(Int16, 100.5)) == 100
    @test Int64(unsafe_trunc(Int8, 127.9)) == 127

    # === Edge cases (unsafe - no error checking) ===
    # Note: unsafe_trunc doesn't check for overflow or NaN
    # These tests verify basic functionality
    @test unsafe_trunc(Int64, 0.0) == 0
    @test unsafe_trunc(Int64, -0.0) == 0
    @test unsafe_trunc(Int64, 1.5) == 1
    @test unsafe_trunc(Int64, -1.5) == -1
end

true
