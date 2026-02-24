# Test numerator accessor function

using Test

@testset "numerator accessor function" begin
    r = 3 // 7
    @test (Float64(numerator(r))) == 3.0
end

true  # Test passed
