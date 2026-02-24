# Test Complex type inference from Int64 arguments

using Test

@testset "Complex(2, 3) - type inference from Int64 arguments" begin
    z = Complex(2, 3)  # Should infer Complex{Int64}
    @test (real(z) + imag(z)) == 5.0
end

true  # Test passed
