# Test oftype function - convert to type of reference

using Test

@testset "oftype - convert to type of reference value" begin

    x = 1.5
    y = 2

    # oftype(x, y) converts y to type of x
    result = oftype(x, y)
    @assert typeof(result) == Float64
    @assert result == 2.0

    # oftype with same type
    z = 3.0
    result2 = oftype(x, z)
    @assert result2 == 3.0

    @test (true)
end

true  # Test passed
