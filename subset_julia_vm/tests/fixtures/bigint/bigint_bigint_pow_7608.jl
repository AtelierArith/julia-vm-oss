using Test

pow_bigint_bigint_7608(x, n) = x ^ n

@testset "BigInt ^ BigInt exponent (Issue #7608)" begin
    @test big(2) ^ big(3) == big(8)
    @test typeof(big(2) ^ big(3)) === BigInt
    @test big(2) ^ big(64) == big(18446744073709551616)
    @test typeof(big(2) ^ big(64)) === BigInt

    @test big(5) ^ big(0) == big(1)
    @test big(0) ^ big(0) == big(1)
    @test big(0) ^ big(5) == big(0)
    @test big(1) ^ big(100) == big(1)

    @test big(-2) ^ big(4) == big(16)
    @test big(-2) ^ big(3) == big(-8)

    n = big(3)
    @test pow_bigint_bigint_7608(big(2), n) == big(8)
    @test typeof(pow_bigint_bigint_7608(big(2), n)) === BigInt

    @test_throws DomainError big(2) ^ big(-1)
end

true
