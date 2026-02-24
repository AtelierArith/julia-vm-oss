# Test dynamic type constructor T(x) syntax
# This tests the T(x) pattern used in Julia

using Test

function my_convert(T, x)
    T(x)
end

@testset "T(x) dynamic type constructor with function parameter" begin

    # Dynamic conversion - function parameter

    # Test: convert Int64 to Float64 (exact conversion)
    result = my_convert(Float64, 3)
    @test (result) == 3.0
end

true  # Test passed
