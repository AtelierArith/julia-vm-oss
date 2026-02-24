# axes: get index range for 2D array dimension

using Test

@testset "axes(matrix) returns tuple of ranges for each dimension" begin
    mat = zeros(2, 3)
    ax1 = axes(mat, 1)
    ax2 = axes(mat, 2)
    @test (length(ax1) * length(ax2)) == 6.0
end

true  # Test passed
