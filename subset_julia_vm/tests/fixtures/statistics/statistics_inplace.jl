# Test in-place statistics functions and quantile with vector of probabilities (Issue #2161)

using Test
using Statistics

@testset "median! - in-place median" begin
    # Odd-length array
    v1 = [5.0, 1.0, 3.0]
    @test median!(v1) == 3.0
    # v1 is now sorted in-place
    @test v1 == [1.0, 3.0, 5.0]

    # Even-length array
    v2 = [4.0, 2.0, 1.0, 3.0]
    @test median!(v2) == 2.5
    @test v2 == [1.0, 2.0, 3.0, 4.0]

    # Single element
    @test median!([42.0]) == 42.0
end

@testset "quantile! - in-place quantile" begin
    # Scalar probability
    v1 = [5.0, 1.0, 3.0, 2.0, 4.0]
    @test quantile!(v1, 0.5) == 3.0
    # v1 is now sorted
    @test v1 == [1.0, 2.0, 3.0, 4.0, 5.0]

    # Boundary values
    v2 = [3.0, 1.0, 2.0]
    @test quantile!(v2, 0.0) == 1.0

    v3 = [3.0, 1.0, 2.0]
    @test quantile!(v3, 1.0) == 3.0
end

@testset "quantile! with vector of probabilities" begin
    v = [5.0, 1.0, 3.0, 2.0, 4.0]
    result = quantile!(v, [0.0, 0.25, 0.5, 0.75, 1.0])
    @test result[1] == 1.0
    @test result[2] == 2.0
    @test result[3] == 3.0
    @test result[4] == 4.0
    @test result[5] == 5.0
end

@testset "quantile with vector of probabilities (non-destructive)" begin
    data = [5.0, 1.0, 3.0, 2.0, 4.0]
    result = quantile(data, [0.25, 0.5, 0.75])
    @test result[1] == 2.0
    @test result[2] == 3.0
    @test result[3] == 4.0
    # Original data is NOT modified
    @test data[1] == 5.0
end

true
