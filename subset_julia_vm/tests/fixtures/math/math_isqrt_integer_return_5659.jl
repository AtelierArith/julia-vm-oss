using Test

# Issue #5659: `isqrt(n)` (integer square root) must return an integer of the SAME
# type as `n`, not a `Float64`. The value is the largest `m` with `m*m <= n`.
# sjulia previously returned `floor(sqrt(n))` (a Float64), so `isqrt(17)` gave
# `4.0 :: Float64` instead of `4 :: Int64`.

@testset "isqrt returns the integer type of its argument (Issue #5659)" begin
    @test isqrt(17) === 4
    @test isqrt(16) === 4
    @test isqrt(0) === 0
    @test isqrt(1) === 1
    @test isqrt(99) === 9
    @test isqrt(100) === 10
    @test typeof(isqrt(17)) === Int64

    # Exactness near boundaries (the float estimate is corrected without overflow).
    @test isqrt(10^12) === 10^6
    @test isqrt(typemax(Int64)) === 3037000499

    # Narrow / unsigned integer types are preserved.
    @test isqrt(Int8(16)) === Int8(4)
    @test isqrt(UInt16(1000)) === UInt16(31)
    @test isqrt(UInt8(200)) === UInt8(14)
end

@testset "isqrt of a negative integer throws DomainError (Issue #5659)" begin
    @test_throws DomainError isqrt(-1)
    @test_throws DomainError isqrt(-100)
end

true
