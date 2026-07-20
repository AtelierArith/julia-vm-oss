# Zero-valued BigFloat values must report their allocation precision.

using Test

p64 = setprecision(BigFloat, 64) do
    (
        precision(BigFloat(0.0)),
        precision(zero(BigFloat)),
        precision(BigFloat(0)),
        precision(BigFloat(2) - BigFloat(2)),
        precision(floor(BigFloat("0.5"))),
        precision(trunc(BigFloat("-0.5"))),
        precision(sqrt(BigFloat(0))),
        precision(BigFloat(Inf)),
        precision(BigFloat(NaN)),
    )
end

p128 = setprecision(BigFloat, 128) do
    (
        precision(BigFloat(-0.0)),
        precision(BigFloat(0) + BigFloat(0)),
        precision(BigFloat(0) / BigFloat(Inf)),
        precision(nextfloat(zero(BigFloat))),
        precision(prevfloat(zero(BigFloat))),
    )
end

@testset "BigFloat zero precision (Issue #9599)" begin
    @test all(==(64), p64)
    @test all(==(128), p128)
    @test precision(BigFloat(0)) == precision(BigFloat)
end

true
