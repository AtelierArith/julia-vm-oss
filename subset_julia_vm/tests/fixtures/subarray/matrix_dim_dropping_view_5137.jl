# Issue #5137: view(A, i, idx) where one index is a scalar Int drops that
# dimension, producing a 1-D SubArray (a row or column slice). The
# range×range / colon combinations already worked; this adds the
# dimension-dropping mixes (scalar Int with Colon or UnitRange) so
# view(matrix, scalar, idx) and view(matrix, idx, scalar) match upstream
# Julia: a 1-D parent-reflecting view (ndims 1, vector-shaped, read/write
# flows to the parent).

using Test

@testset "view dimension-dropping scalar index (Issue #5137)" begin
    A = [1 2 3; 4 5 6]

    # scalar row + colon -> the i-th row as a 1-D view
    r = view(A, 1, :)
    @test ndims(r) == 1
    @test length(r) == 3
    @test size(r) == (3,)
    @test collect(r) == [1, 2, 3]
    @test r[2] == 2
    @test r isa AbstractVector

    # colon + scalar col -> the j-th column as a 1-D view
    c = view(A, :, 2)
    @test ndims(c) == 1
    @test length(c) == 2
    @test size(c) == (2,)
    @test collect(c) == [2, 5]
    @test c[1] == 2

    # scalar row + range
    rr = view(A, 1, 2:3)
    @test ndims(rr) == 1
    @test length(rr) == 2
    @test collect(rr) == [2, 3]

    # range + scalar col
    cc = view(A, 1:2, 3)
    @test ndims(cc) == 1
    @test length(cc) == 2
    @test collect(cc) == [3, 6]

    # writes through a dimension-dropping view reflect into the parent
    r[1] = 100
    @test A[1, 1] == 100
    c[2] = 50
    @test A[2, 2] == 50
    cc[1] = 7
    @test A[1, 3] == 7

    # a Float matrix column view round-trips and aliases the parent
    F = [1.0 2.0; 3.0 4.0]
    fc = view(F, :, 1)
    @test collect(fc) == [1.0, 3.0]
    fc[2] = 9.0
    @test F[2, 1] == 9.0

    # iteration / sum over a 1-D dimension-dropping view
    @test sum(view(A, :, 1)) == 100 + 4
end

true
