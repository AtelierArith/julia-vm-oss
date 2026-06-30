# Issue #5137: a 3-D range view `view(A::Array{T,3}, r1, r2, r3)` returns a 3-D
# SubArray whose reads and writes flow through to the parent, mirroring the 1-D
# and 2-D range views. The shared column-major linear load/store maps a
# three-index `indices` tuple into `parent[i1, i2, i3]`.

using Test

@testset "3-D range view (Issue #5137)" begin
    A = reshape(collect(1:24), 2, 3, 4)

    v = view(A, 1:2, 1:2, 1:3)
    @test ndims(v) == 3
    @test size(v) == (2, 2, 3)
    @test length(v) == 12

    # Cartesian element access matches the parent
    @test v[1, 1, 1] == A[1, 1, 1]
    @test v[2, 2, 2] == A[2, 2, 2]
    @test v[2, 2, 3] == A[2, 2, 3]

    # collect copies into a 3-D Array{T,3} with the right shape and values
    full = view(A, 1:2, 1:3, 1:4)
    c = collect(full)
    @test c isa Array{Int64,3}
    @test size(c) == (2, 3, 4)
    @test c == A

    # a strict sub-box round-trips
    sub = collect(view(A, 1:1, 2:3, 1:2))
    @test size(sub) == (1, 2, 2)
    @test sub[1, 1, 1] == A[1, 2, 1]
    @test sub[1, 2, 2] == A[1, 3, 2]

    # linear iteration / sum over the view (the 2x2x2 sub-box of reshape(1:24,2,3,4))
    @test sum(view(A, 1:2, 1:2, 1:2)) == 44

    # writes through the view reflect into the parent (linear and Cartesian)
    w = view(A, 1:2, 1:2, 1:2)
    w[1] = 999
    @test A[1, 1, 1] == 999
    w[2, 2, 2] = 777
    @test A[2, 2, 2] == 777
    @test w[2, 2, 2] == 777
end

true
