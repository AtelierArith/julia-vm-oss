# Test Complex{Float64} + Int arithmetic

using Test

@testset "Complex{Float64} + Int - complex number added to integer" begin
    c = 2.0 + 3.0im
    result = c + 5  # Should be 7.0 + 3.0im
    @test (real(result)) == 7.0
end

true  # Test passed
