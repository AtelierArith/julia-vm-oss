using Test

pow_int_bigint_9352(x, n) = x ^ n

@testset "machine-integer base ^ BigInt exponent (Issue #9352)" begin
    # Int64 base with BigInt exponent promotes the base to BigInt,
    # mirroring upstream `^(x::Integer, y::BigInt) = bigint_pow(BigInt(x), y)`.
    @test 2 ^ big(3) == 8
    @test 2 ^ big(3) == big(8)
    @test typeof(2 ^ big(3)) === BigInt

    # Other machine-integer widths promote too.
    @test Int8(2) ^ big(3) == big(8)
    @test typeof(Int8(2) ^ big(3)) === BigInt
    @test UInt8(2) ^ big(3) == big(8)
    @test typeof(UInt8(2) ^ big(3)) === BigInt

    # Beyond Int64 range: exact BigInt arithmetic.
    @test 2 ^ big(64) == big(18446744073709551616)

    # Negative base and zero exponent.
    @test (-2) ^ big(3) == big(-8)
    @test 5 ^ big(0) == big(1)

    # Bool base keeps a Bool result (upstream `^(x::Bool, y::BigInt)`
    # is `Base.power_by_squaring(x, y)`).
    @test true ^ big(3) === true
    @test false ^ big(0) === true
    @test false ^ big(2) === false
    @test true ^ big(-1) === true

    # Through a generic function (dynamic dispatch path).
    @test pow_int_bigint_9352(2, big(3)) == big(8)
    @test typeof(pow_int_bigint_9352(2, big(3))) === BigInt

    # Regressions: the previously-working combinations stay intact.
    @test big(2) ^ 3 == big(8)
    @test typeof(big(2) ^ 3) === BigInt
    @test big(2) ^ big(3) == big(8)
    @test 2.0 ^ big(3) == 8.0
    @test typeof(2.0 ^ big(3)) === Float64

    # Negative BigInt exponent with an integer base throws DomainError
    # (upstream: `2^big(-1)` -> DomainError).
    @test_throws DomainError 2 ^ big(-1)
    @test_throws DomainError false ^ big(-1)
end

true
