# Test repeat function for arrays

using Test

@testset "Array repeat: repeat(v, n) and repeat(arr, m, n)" begin

    # Test 1: repeat vector n times
    v = [1.0, 2.0, 3.0]
    r1 = repeat(v, 2)
    @assert length(r1) == 6
    @assert r1[1] == 1.0
    @assert r1[4] == 1.0
    @assert sum(r1) == 12.0  # (1+2+3) * 2

    # Test 2: repeat vector with m, n (creates matrix)
    v2 = [1.0, 2.0]
    r2 = repeat(v2, 2, 3)
    # Result should be 4×3 matrix
    @assert size(r2, 1) == 4
    @assert size(r2, 2) == 3
    @assert r2[1, 1] == 1.0
    @assert r2[2, 1] == 2.0
    @assert r2[3, 1] == 1.0

    # Test 3: repeat 2D matrix
    mat = [1.0 2.0; 3.0 4.0]
    r3 = repeat(mat, 2, 2)
    # Result should be 4×4 matrix
    @assert size(r3, 1) == 4
    @assert size(r3, 2) == 4
    @assert r3[1, 1] == 1.0
    @assert r3[3, 3] == 1.0
    @assert r3[4, 4] == 4.0

    # Test 4: single element repeat
    s = [5.0]
    r4 = repeat(s, 3)
    @assert length(r4) == 3
    @assert sum(r4) == 15.0

    @test (true)
end

true  # Test passed
