# Test permutedims function (2D matrices only)
# Note: For 1D vectors, use reshape(v, 1, length(v)) directly

using Test

@testset "permutedims - dimension permutation for vectors and matrices" begin

    # Test 1: 2D matrix transpose (no perm argument)
    mat = zeros(2, 3)
    mat[1, 1] = 1.0
    mat[1, 2] = 2.0
    mat[1, 3] = 3.0
    mat[2, 1] = 4.0
    mat[2, 2] = 5.0
    mat[2, 3] = 6.0
    # mat = [1 2 3; 4 5 6]

    t = permutedims(mat)
    tsz = size(t)
    @assert tsz[1] == 3 "Transpose should swap dimensions (rows)"
    @assert tsz[2] == 2 "Transpose should swap dimensions (cols)"
    @assert t[1, 1] == 1.0
    @assert t[1, 2] == 4.0
    @assert t[2, 1] == 2.0
    @assert t[3, 2] == 6.0

    # Test 2: Explicit permutation (2, 1) - same as transpose
    t2 = permutedims(mat, (2, 1))
    @assert t2[1, 1] == 1.0
    @assert t2[3, 2] == 6.0

    # Test 3: Identity permutation (1, 2) - copy
    c = permutedims(mat, (1, 2))
    csz = size(c)
    @assert csz[1] == 2
    @assert csz[2] == 3
    @assert c[1, 1] == 1.0
    @assert c[2, 3] == 6.0

    # Test 4: Square matrix transpose
    sq = zeros(3, 3)
    sq[1, 1] = 1.0
    sq[1, 2] = 2.0
    sq[1, 3] = 3.0
    sq[2, 1] = 4.0
    sq[2, 2] = 5.0
    sq[2, 3] = 6.0
    sq[3, 1] = 7.0
    sq[3, 2] = 8.0
    sq[3, 3] = 9.0

    tsq = permutedims(sq)
    @assert tsq[1, 2] == 4.0 "sq[2,1] should become tsq[1,2]"
    @assert tsq[2, 1] == 2.0 "sq[1,2] should become tsq[2,1]"
    @assert tsq[3, 1] == 3.0 "sq[1,3] should become tsq[3,1]"

    # Return true to indicate all tests passed
    @test (true)
end

true  # Test passed
