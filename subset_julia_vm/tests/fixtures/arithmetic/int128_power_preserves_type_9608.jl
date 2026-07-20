using Test

pow_identity_9608(x, n) = x ^ n

@testset "Int128 power preserves base type (Issue #9608)" begin
    expected63 = Int128(1) << 63
    expected64 = Int128(1) << 64

    @test typeof(Int128(2)^63) === Int128
    @test Int128(2)^63 === expected63
    @test typeof(Int128(2)^64) === Int128
    @test Int128(2)^64 === expected64
    @test typeof(Int128(2)^64 + Int128(1)) === Int128
    @test Int128(2)^64 + Int128(1) === expected64 + Int128(1)

    @test typeof(pow_identity_9608(Int128(2), 64)) === Int128
    @test pow_identity_9608(Int128(2), 64) === expected64
    @test typeof(pow_identity_9608(Int128(2), UInt8(64))) === Int128
    @test pow_identity_9608(Int128(2), UInt8(64)) === expected64

    pow = ^
    @test typeof(pow(Int128(2), 64)) === Int128
    @test pow(Int128(2), 64) === expected64
end

@testset "UInt128 power remains type-preserving" begin
    expected64 = UInt128(1) << 64

    @test typeof(UInt128(2)^64) === UInt128
    @test UInt128(2)^64 === expected64
    @test typeof(UInt128(2)^64 + UInt128(1)) === UInt128
    @test UInt128(2)^64 + UInt128(1) === expected64 + UInt128(1)
end

true
