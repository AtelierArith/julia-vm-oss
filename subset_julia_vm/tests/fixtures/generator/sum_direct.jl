# Generator with sum directly (without collect)
# sum(x^2 for x in 1:10) = 1+4+9+16+25+36+49+64+81+100 = 385

using Test

@testset "Generator with sum directly (no collect)" begin
    @test sum(x^2 for x in 1:10) == 385
end

true  # Test passed
