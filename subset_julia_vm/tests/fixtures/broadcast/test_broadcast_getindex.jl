using Test

# Test _broadcast_getindex function (Issue #2538)

@testset "_broadcast_getindex" begin
    # Scalar: always returns the scalar regardless of index
    @test _broadcast_getindex(42, 1) == 42
    @test _broadcast_getindex(42, 5) == 42
    @test _broadcast_getindex(3.14, 1) == 3.14

    # Bool scalar
    @test _broadcast_getindex(true, 1) == true
    @test _broadcast_getindex(false, 3) == false

    # Tuple: index into the tuple
    @test _broadcast_getindex((10, 20, 30), 1) == 10
    @test _broadcast_getindex((10, 20, 30), 2) == 20
    @test _broadcast_getindex((10, 20, 30), 3) == 30

    # Array: direct indexing for 1D
    A = [100, 200, 300]
    @test _broadcast_getindex(A, 1) == 100
    @test _broadcast_getindex(A, 2) == 200
    @test _broadcast_getindex(A, 3) == 300

    # _getindex: collect values from args tuple
    args = ([1, 2, 3], [4, 5, 6])
    vals = _getindex(args, 2)
    @test vals[1] == 2
    @test vals[2] == 5
end

true
