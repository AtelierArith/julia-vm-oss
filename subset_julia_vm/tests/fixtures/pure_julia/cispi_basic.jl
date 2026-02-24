# Test: cispi function - cis(pi*x)
# Expected: true

using Test

@testset "cispi(x) - returns cos(pi*x) + im*sin(pi*x)" begin

    z = cispi(1.0)  # cis(pi) = -1 + 0*im
    @test (abs(z.re + 1.0) < 1e-10 && abs(z.im) < 1e-10)
end

true  # Test passed
