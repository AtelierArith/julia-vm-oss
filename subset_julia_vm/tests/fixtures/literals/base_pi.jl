# Test Base.pi access
# Julia exports pi from Base.MathConstants

using Test

@testset "Access pi constant via Base.pi (same as pi)" begin
    x = Base.pi
    y = pi
    # Return 1.0 if they are equal, 0.0 otherwise
    @test (Float64(x == y)) == 1.0
end

true  # Test passed
