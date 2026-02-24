# Test: reim function - decompose complex
# Expected: true

using Test

@testset "reim(z) - returns (real(z), imag(z)) tuple" begin

    r, i = reim(Complex{Float64}(3.0, 4.0))
    @test (r == 3.0 && i == 4.0)
end

true  # Test passed
