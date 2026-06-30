using Test

# Issue #5816: linear (single-index) getindex on a 2D view returned the parent's
# i-th LINEAR element (a contiguous `offset + i` assumption valid only for a 1D
# view). A 2D view is non-contiguous, so the column-major linear index must be
# split into (row, col) and routed through the per-dimension parent indices —
# the same mapping the Cartesian `v[i, j]` already used.

@testset "linear getindex on a 2D view (Issue #5816)" begin
    A = reshape(collect(1:9), 3, 3)
    v = view(A, 1:2, 2:3)
    @test v[1] == 4
    @test v[2] == 5
    @test v[3] == 7
    @test v[4] == 8
    # Cartesian indexing (control, already correct)
    @test v[1, 1] == 4
    @test v[2, 2] == 8

    # Linear iteration / reduction over a 2D view
    @test [v[i] for i in 1:length(v)] == [4, 5, 7, 8]
    s = 0
    for i in 1:length(v)
        s += v[i]
    end
    @test s == 24

    # Float 2D view
    Af = reshape(collect(1.0:9.0), 3, 3)
    vf = view(Af, 2:3, 1:2)
    @test vf[1] == 2.0
    @test vf[2] == 3.0
    @test vf[3] == 5.0
    @test vf[4] == 6.0

    # 1D view linear indexing unchanged
    a = [10, 20, 30, 40, 50]
    u = view(a, 2:4)
    @test u[1] == 20
    @test u[3] == 40
end

true
