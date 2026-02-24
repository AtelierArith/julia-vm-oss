# Test cross (cross product) function from LinearAlgebra

using Test
using LinearAlgebra

@testset "cross - cross product of 3D vectors" begin

    # Standard basis vectors
    i = [1.0, 0.0, 0.0]
    j = [0.0, 1.0, 0.0]
    k = [0.0, 0.0, 1.0]

    # i × j = k
    r1 = cross(i, j)
    @assert r1[1] == 0.0 "i × j: x component"
    @assert r1[2] == 0.0 "i × j: y component"
    @assert r1[3] == 1.0 "i × j: z component"

    # j × k = i
    r2 = cross(j, k)
    @assert r2[1] == 1.0 "j × k: x component"
    @assert r2[2] == 0.0 "j × k: y component"
    @assert r2[3] == 0.0 "j × k: z component"

    # k × i = j
    r3 = cross(k, i)
    @assert r3[1] == 0.0 "k × i: x component"
    @assert r3[2] == 1.0 "k × i: y component"
    @assert r3[3] == 0.0 "k × i: z component"

    # Anti-commutativity: a × b = -(b × a)
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    ab = cross(a, b)
    ba = cross(b, a)
    @assert ab[1] == -ba[1] "anti-commutativity: x"
    @assert ab[2] == -ba[2] "anti-commutativity: y"
    @assert ab[3] == -ba[3] "anti-commutativity: z"

    # Parallel vectors: a × a = 0
    aa = cross(a, a)
    @assert aa[1] == 0.0 "parallel: x"
    @assert aa[2] == 0.0 "parallel: y"
    @assert aa[3] == 0.0 "parallel: z"

    # Specific calculation: [1,2,3] × [4,5,6] = [-3, 6, -3]
    @assert ab[1] == -3.0 "cross product: x = 2*6 - 3*5 = -3"
    @assert ab[2] == 6.0 "cross product: y = 3*4 - 1*6 = 6"
    @assert ab[3] == -3.0 "cross product: z = 1*5 - 2*4 = -3"

    @test (true)
end

true  # Test passed
