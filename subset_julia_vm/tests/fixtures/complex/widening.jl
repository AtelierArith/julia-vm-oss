# Test Complex type widening (Int64 + Float64 -> Float64)

using Test

@testset "Complex(1, 2.5) - type widening Int64 + Float64 -> Float64" begin
    z = Complex(1, 2.5)  # Should widen to Complex{Float64}
    @test isapprox((real(z) + imag(z)), 3.5)
end

true  # Test passed
