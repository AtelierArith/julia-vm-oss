# Test Int + Complex{Float64} arithmetic

using Test

@testset "Int + Complex{Float64} - integer added to complex number" begin
    c = 2.0 + 3.0im
    result = 1 + c  # Should be 3.0 + 3.0im
    @test (real(result)) == 3.0
end

true  # Test passed
