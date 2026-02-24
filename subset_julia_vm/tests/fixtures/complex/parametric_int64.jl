# Test Complex{Int64} explicit type parameter

using Test

@testset "Complex{Int64}(5, 6) - explicit Int64 type parameter" begin
    z = Complex{Int64}(5, 6)
    @test (real(z) + imag(z)) == 11.0
end

true  # Test passed
