# axes: get index range for dimension
# Expected: 3 (Int)

using Test

@testset "axes(arr, 1) returns index range for dimension" begin
    arr = [1.0, 2.0, 3.0]
    ax = axes(arr, 1)
    @test (length(ax)) == 3
end

true  # Test passed
