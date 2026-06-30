# Issue #4814: scalars in upstream Julia behave as 0-dimensional
# collections — `length(x) == 1`, `x[1] == x`. sjulia already
# supported `length(10) == 1`, but scalar `getindex` was rejected
# with `Type error: indexing not supported for I64(10)`. The most
# visible downstream impact is `vcat(scalar, vec, ...)` — a common
# Julia idiom — whose Pure-Julia body indexes each arg via
# `args[i][j]`. This also forced the array-literal splat lowering
# (`[a, v..., b]`, PR #4813) to wrap each non-splat element in a
# 1-element vector to dodge the scalar-indexing path; with scalar
# `getindex` supported, that workaround can be simplified.
#
# Fix: in `subset_julia_vm/src/vm/exec/array_index.rs::IndexLoad`,
# the fallthrough `other =>` arm now first checks
# `is_scalar_indexable_value(&other)` — true for `Number` and
# `AbstractChar` subtypes (matching upstream's type hierarchy) — and
# returns `other` for `x[1]` or raises `BoundsError` otherwise.
# `Symbol`, `Nothing`, and `Missing` are excluded because upstream
# Julia also raises `MethodError` on `:foo[1]` etc.

using Test

@testset "scalar getindex returns the scalar (Issue #4814)" begin
    @test 10[1] == 10
    @test 3.14[1] == 3.14
    @test true[1] == true
    @test 'A'[1] == 'A'
    @test Int32(5)[1] == 5
end

@testset "scalar getindex preserves type (Issue #4814)" begin
    @test typeof(10[1]) === Int64
    @test typeof(3.14[1]) === Float64
    @test typeof(true[1]) === Bool
    @test typeof('A'[1]) === Char
    @test typeof(Int32(5)[1]) === Int32
end

@testset "scalar getindex out-of-bounds raises (Issue #4814)" begin
    # The shape is `(1,)` for any scalar; only index 1 is valid.
    @test_throws Exception 10[2]
    @test_throws Exception 10[0]
    @test_throws Exception 3.14[5]
end

@testset "length(scalar) == 1 regression guard (Issue #4814)" begin
    # Already worked before #4814 but is the type-level partner of
    # scalar getindex — pin it so the pair stays in sync. `length('A')`
    # is excluded because it surfaces an orthogonal pre-existing
    # limitation (`length not defined for Char`) that is out of scope
    # for this PR.
    @test length(10) == 1
    @test length(3.14) == 1
    @test length(true) == 1
end

@testset "vcat(scalar, ...) — the headline downstream case (Issue #4814)" begin
    @test vcat(10, [1, 2], 20) == [10, 1, 2, 20]
    @test vcat(1, 2, 3) == [1, 2, 3]
    @test vcat([1], 2, [3, 4], 5) == [1, 2, 3, 4, 5]
    @test vcat(1.0, 2.0, [3.0, 4.0]) == [1.0, 2.0, 3.0, 4.0]
end

true
