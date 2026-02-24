# Test eltype function
# Returns sum of successful checks (1 if eltype matches expected)

using Test

@testset "eltype function returns element type of collections" begin

    result = 0.0

    # Array element types - Float64 array
    if eltype([1.0, 2.0, 3.0]) === Float64
        result += 1.0
    end

    # Integer array
    if eltype([1, 2, 3]) === Int64
        result += 1.0
    end

    # Tuple element type (homogeneous)
    if eltype((1, 2, 3)) === Int64
        result += 1.0
    end

    # String element type
    if eltype("hello") === Char
        result += 1.0
    end

    @test (result) == 4.0
end

true  # Test passed
