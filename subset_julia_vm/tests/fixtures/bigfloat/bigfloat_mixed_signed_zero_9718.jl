using Test

setprecision(256)

@testset "Mixed BigFloat signed-zero residuals (Issue #9718)" begin
    @test typeof(BigFloat(-0.0) + Int128(0)) === BigFloat
    @test repr(BigFloat(-0.0) + Int128(0)) == "0.0"
    @test repr(UInt128(0) - BigFloat("0.0")) == "0.0"

    @test repr(Int64(-1) - BigFloat("-1.0")) == "-0.0"
    @test repr(big(-1) - BigFloat("-1.0")) == "-0.0"

    @test repr(BigFloat(-0.0) + 0) == "-0.0"
    @test repr(false - BigFloat("0.0")) == "0.0"
    @test repr(Int8(0) - BigFloat("0.0")) == "-0.0"
end

true
