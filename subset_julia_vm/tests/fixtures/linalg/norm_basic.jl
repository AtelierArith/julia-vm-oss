# Test norm function from LinearAlgebra

using Test
using LinearAlgebra

@testset "norm - L1, L2, Inf norms of vectors" begin

    # L2 norm (Euclidean) - default
    v = [3.0, 4.0]
    @assert norm(v) == 5.0 "L2 norm of [3, 4]"

    # Unit vector
    u = [1.0, 0.0, 0.0]
    @assert norm(u) == 1.0 "norm of unit vector"

    # L1 norm (Manhattan)
    w = [1.0, -2.0, 3.0]
    n1 = norm(w, 1)
    @assert n1 == 6.0 "L1 norm: |1| + |-2| + |3| = 6"

    # L2 norm explicit
    n2 = norm(w, 2)
    @assert isapprox(n2, sqrt(14.0)) "L2 norm: sqrt(1 + 4 + 9)"

    # Inf norm (max)
    ninf = norm(w, Inf)
    @assert ninf == 3.0 "Inf norm: max(|1|, |-2|, |3|) = 3"

    @test (true)
end

true  # Test passed
