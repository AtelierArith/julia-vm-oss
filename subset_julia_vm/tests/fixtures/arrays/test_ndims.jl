# Test ndims function

using Test

@testset "ndims - number of dimensions" begin
    # 1D array (Vector)
    v = [1, 2, 3]
    @test ndims(v) == 1

    # 2D array (Matrix)
    m = [1 2 3; 4 5 6]
    @test ndims(m) == 2

    # Using zeros/ones
    @test ndims(zeros(5)) == 1
    @test ndims(zeros(3, 4)) == 2
    @test ndims(ones(2, 3)) == 2
end

true
