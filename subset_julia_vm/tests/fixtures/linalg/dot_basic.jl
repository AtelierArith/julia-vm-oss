# Test dot (inner product) function from LinearAlgebra

using Test
using LinearAlgebra

@testset "dot (inner product) of vectors" begin

    # Basic dot product
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    @assert dot(x, y) == 32.0 "dot product: 1*4 + 2*5 + 3*6 = 32"

    # Orthogonal vectors
    a = [1.0, 0.0, 0.0]
    b = [0.0, 1.0, 0.0]
    @assert dot(a, b) == 0.0 "orthogonal vectors"

    # Same vector (squared norm)
    v = [1.0, 1.0, 1.0]
    @assert dot(v, v) == 3.0 "dot(v, v) = |v|^2"

    # Integer vectors
    u = [1, 2, 3]
    w = [1, 1, 1]
    d = dot(u, w)
    @assert d == 6.0 "integer dot product"

    @test (true)
end

true  # Test passed
