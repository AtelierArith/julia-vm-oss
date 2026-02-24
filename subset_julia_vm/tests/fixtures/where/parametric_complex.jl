# Test parametric method dispatch with Complex{T}
# The method should match Complex{Float64} because Float64 <: Real

using Test

function get_real(z::Complex{T}) where T<:Real
    return z.re
end

@testset "Parametric dispatch: Complex{Float64} matches Complex{T} where T<:Real" begin

    z = Complex{Float64}(2.0, 3.0)
    @test (get_real(z)) == 2.0
end

true  # Test passed
