# Issue #5137: `reshape(view(...), n)` produces a 1-D ReshapedArray whose parent
# is a SubArray. `collect`, `map`, and `sum` over it failed ("iterate: no method
# matching ... ReshapedArray", and the runtime collect iterator path does not
# recognize the ReshapedArray struct). Linear iterate/collect/map (via the
# element getindex, which delegates to the SubArray parent) now match upstream.

using Test

@testset "reshape(view(...), n) iteration / collect / map (Issue #5137)" begin
    A = [1 2 3; 4 5 6]
    v = view(A, 1:2, 2:3)        # the 2x2 sub-box [2 3; 5 6]
    r = reshape(v, 4)            # 1-D ReshapedArray over the SubArray

    @test length(r) == 4
    @test r[1] == 2
    @test r[4] == 6

    # collect reads through to the parent in column-major order
    @test collect(r) == [2, 5, 3, 6]

    # map and sum drive iteration over the reshaped view
    @test map(x -> x * 10, r) == [20, 50, 30, 60]
    @test sum(r) == 16

    # the parent matrix is untouched by these (copying) operations
    @test A == [1 2 3; 4 5 6]

    # reshaping a 1-D range view round-trips its elements
    w = view(collect(10:10:60), 1:6)
    @test collect(reshape(w, 6)) == [10, 20, 30, 40, 50, 60]
end

true
