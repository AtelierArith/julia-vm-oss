using Test

@testset "Fixed-width integer boundary div/rem/mod (Issue #9716)" begin
    @test cld(false, typemin(Int128)) === Int128(0)
    @test rem(Int128(3), typemin(Int128)) === Int128(3)

    q = cld(typemin(Int64), UInt64(5))
    @test typeof(q) === Int64
    @test q === -1844674407370955161

    m = mod(typemin(Int64), UInt64(5))
    @test typeof(m) === UInt64
    @test m === UInt64(2)

    r = rem(typemax(Int128), UInt128(5))
    @test typeof(r) === Int128
    @test r === Int128(2)
end

@testset "Unary negation wraps typemin (Issue #9838)" begin
    @test -typemin(Int64) === typemin(Int64)
end

true
