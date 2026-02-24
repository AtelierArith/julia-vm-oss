using Test

@testset "RoundingMode constants" begin
    @test isa(RoundNearest, RoundingMode)
    @test isa(RoundToZero, RoundingMode)
    @test isa(RoundUp, RoundingMode)
    @test isa(RoundDown, RoundingMode)
end

true
