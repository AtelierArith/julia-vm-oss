# Test denominator accessor function

using Test

@testset "denominator accessor function" begin
    r = 3 // 7
    @test (Float64(denominator(r))) == 7.0
end

true  # Test passed
