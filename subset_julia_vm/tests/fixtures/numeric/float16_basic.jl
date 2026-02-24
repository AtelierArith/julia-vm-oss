# Float16 special values and type test

using Test

@testset "Float16 special values" begin
    # Test isinf and isnan work with Float16 special values
    @test isinf(Inf16)
    @test isnan(NaN16)
    @test !isnan(Inf16)
    @test !isinf(NaN16)
end

true
