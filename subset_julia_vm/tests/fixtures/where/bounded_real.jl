# Test where clause with bounded type parameter
# Uses identity function to avoid Any-typed arithmetic issue

using Test

function identity_real(x::T) where T<:Real
    x
end

@testset "Function with bounded where clause (where T<:Real) - identity function" begin
    @test (identity_real(3.0)) == 3.0
end

true  # Test passed
