using Test

pow_bigint_float_9653(x, y) = x ^ y

@testset "BigInt power with Float exponent returns BigFloat (Issue #9653)" begin
    r = big(2)^1.5
    @test typeof(r) === BigFloat
    @test r == BigFloat(2)^BigFloat(1.5)

    r32 = big(2)^Float32(1.5)
    @test typeof(r32) === BigFloat
    @test r32 == BigFloat(2)^BigFloat(Float32(1.5))

    r16 = big(2)^Float16(1.5)
    @test typeof(r16) === BigFloat
    @test r16 == BigFloat(2)^BigFloat(Float16(1.5))

    @test typeof(pow_bigint_float_9653(big(9), 0.5)) === BigFloat
    @test pow_bigint_float_9653(big(9), 0.5) == BigFloat(3)
end

true
