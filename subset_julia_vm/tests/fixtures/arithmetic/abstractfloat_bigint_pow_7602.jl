using Test

pow_bigint_exp_7602(x, n) = x ^ n

@testset "AbstractFloat ^ BigInt preserves the base float type (Issue #7602)" begin
    @test 2.0 ^ big(3) === 8.0
    @test typeof(2.0 ^ big(3)) === Float64
    @test 2.0 ^ big(-3) === 0.125
    @test typeof(2.0 ^ big(-3)) === Float64

    @test 2.0f0 ^ big(3) === 8.0f0
    @test typeof(2.0f0 ^ big(3)) === Float32
    @test 2.0f0 ^ big(-3) === 0.125f0
    @test typeof(2.0f0 ^ big(-3)) === Float32

    @test Float16(2) ^ big(3) === Float16(8)
    @test typeof(Float16(2) ^ big(3)) === Float16
    @test Float16(2) ^ big(-3) === Float16(0.125)
    @test typeof(Float16(2) ^ big(-3)) === Float16

    n = big(3)
    @test pow_bigint_exp_7602(2.0, n) === 8.0
    @test pow_bigint_exp_7602(2.0f0, n) === 8.0f0
    @test pow_bigint_exp_7602(Float16(2), n) === Float16(8)
end

true
