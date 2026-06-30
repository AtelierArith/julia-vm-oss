# Issue #5816: linear getindex/setindex! on a 2D view (SubArray) must read/write
# the element the column-major linear index designates, going through the view's
# per-dimension `indices` and parent — not the 1D-contiguous `offset + i` layout.
# Previously `v[i] = x` on a 2D view errored ("SubArray parent must be an Array")
# or corrupted the binding (the routed store left the value, not the collection,
# on the stack so the post-IndexStore StoreBack clobbered `v`).

using Test

@testset "2D view linear getindex/setindex! (Issue #5816)" begin
    A = reshape(collect(1:9), 3, 3)
    v = view(A, 1:2, 2:3)            # 2x2 view: column-major elements 4,5,7,8

    # linear getindex (already correct; guard against regression)
    @test v[1] == 4
    @test v[2] == 5
    @test v[3] == 7
    @test v[4] == 8

    # linear setindex! writes through to the parent at the right cell.
    v[1] = 100                        # -> A[1,2]
    v[4] = 200                        # -> A[2,3]
    @test A[1, 2] == 100
    @test A[2, 3] == 200
    @test A == [1 100 7; 2 5 200; 3 6 9]

    # the view binding survives the store (no StoreBack clobber) and reads back right.
    @test v[1] == 100
    @test v[4] == 200
    @test v[2] * 2 == 10              # untouched element, no leftover on the stack
    @test typeof(v[1]) == Int64       # value stays Int (not coerced to Float64)
end

@testset "1D view linear setindex! regression (Issue #5816)" begin
    B = collect(1:5)
    w = view(B, 2:4)
    w[1] = 10
    w[2] = 20
    @test B == [1, 10, 20, 4, 5]
    @test w[1] == 10
end

@testset "Float 2D view linear setindex! (Issue #5816)" begin
    C = reshape(collect(1.0:9.0), 3, 3)
    vc = view(C, 1:2, 2:3)
    vc[3] = 99.0
    @test C[1, 3] == 99.0
end

true
