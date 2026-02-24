# Test: cis function - complex exponential
# Expected: true

using Test

@testset "cis(x) - returns cos(x) + im*sin(x)" begin

    z = cis(0.0)
    @test (abs(z.re - 1.0) < 1e-10 && abs(z.im - 0.0) < 1e-10)
end

true  # Test passed
