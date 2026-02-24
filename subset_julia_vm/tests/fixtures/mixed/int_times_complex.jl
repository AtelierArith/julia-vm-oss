# Test Int * Complex{Float64} arithmetic

using Test

@testset "Int * Complex{Float64} - integer times complex number" begin
    c = 1.0 + 2.0im
    result = 3 * c  # Should be 3.0 + 6.0im
    @test (imag(result)) == 6.0
end

true  # Test passed
