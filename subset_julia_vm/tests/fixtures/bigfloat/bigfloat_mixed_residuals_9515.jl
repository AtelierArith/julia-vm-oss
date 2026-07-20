# BigFloat mixed-operand residuals from Issue #9515.

using Test

@testset "mixed BigFloat exact-zero add/sub signs (Issue #9515)" begin
    @test signbit(BigFloat(-0.0) + big(0))
    @test signbit(big(0) + BigFloat(-0.0))
    @test signbit(BigFloat(-0.0) + false)
    @test signbit(false + BigFloat(-0.0))

    @test signbit(BigFloat(-0.0) - big(0))
    @test !signbit(big(0) - BigFloat(-0.0))
    @test !signbit(BigFloat(0.0) - big(0))
    @test signbit(big(0) - BigFloat(0.0))
end

@testset "mixed BigFloat zero-product signs (Issue #9515)" begin
    @test signbit(BigFloat("2.0") * Float16(-0.0))
    @test signbit(Float16(-0.0) * BigFloat("2.0"))
    @test !isnan(BigFloat(Inf) * false)
    @test BigFloat(Inf) * false == BigFloat(0.0)
    @test !signbit(BigFloat(Inf) * false)
    @test signbit(BigFloat(-Inf) * false)
    @test signbit(BigFloat(-2.0) * false)
    @test !signbit(BigFloat(2.0) * false)
end

@testset "BigFloat promotion keeps large integers exact for mod (Issue #9515)" begin
    y = big(typemax(Int128))
    promoted = promote(BigFloat(-1.0), y)
    @test typeof(promoted[1]) === BigFloat
    @test typeof(promoted[2]) === BigFloat
    @test promoted[1] == BigFloat(-1.0)
    @test promoted[2] == BigFloat(y)
    @test mod(BigFloat(-1.0), y) == BigFloat(y) - BigFloat(1)
    @test string(mod(BigFloat(-1.0), y)) == "1.70141183460469231731687303715884105726e+38"
end

true
