# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/arrays_2d_view_linear_setindex_5816.jl =====
module Agg_arrays_2d_view_linear_setindex_5816
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
end # module Agg_arrays_2d_view_linear_setindex_5816

# ===== source: array/begin_end_multidim.jl =====
module Agg_begin_end_multidim
# Multi-dimensional begin/end indexing (Issue #2349)
# Tests dimension-aware begin/end keyword resolution

using Test

@testset "Multi-dimensional begin/end indexing (Issue #2349)" begin
    # 2x2 matrix
    m = [1 2; 3 4]

    # Basic corner access
    @test m[begin, begin] == 1  # top-left
    @test m[begin, end] == 2    # top-right
    @test m[end, begin] == 3    # bottom-left
    @test m[end, end] == 4      # bottom-right

    # begin/end with arithmetic
    @test m[begin, begin+1] == 2
    @test m[begin+1, begin] == 3
    @test m[end-1+1, end-1+1] == 4

    # 3x3 matrix
    m3 = [1 2 3; 4 5 6; 7 8 9]
    @test m3[begin, begin] == 1
    @test m3[begin, end] == 3
    @test m3[end, begin] == 7
    @test m3[end, end] == 9
    @test m3[begin+1, begin+1] == 5  # center element

    # 2x3 matrix (non-square)
    m23 = [1 2 3; 4 5 6]  # 2 rows, 3 cols
    @test m23[begin, begin] == 1
    @test m23[begin, end] == 3    # end in dim 2 is 3
    @test m23[end, begin] == 4
    @test m23[end, end] == 6

    # 3x2 matrix
    m32 = [1 2; 3 4; 5 6]  # 3 rows, 2 cols
    @test m32[begin, begin] == 1
    @test m32[begin, end] == 2    # end in dim 2 is 2
    @test m32[end, begin] == 5    # end in dim 1 is 3
    @test m32[end, end] == 6
end

@testset "1D array begin/end (regression)" begin
    # Ensure 1D arrays still work correctly
    v = [10, 20, 30, 40, 50]
    @test v[begin] == 10
    @test v[end] == 50
    @test v[begin+1] == 20
    @test v[end-1] == 40
    @test v[begin:end] == [10, 20, 30, 40, 50]
end
end # module Agg_begin_end_multidim

# ===== source: array/begin_indexing.jl =====
module Agg_begin_indexing
# Tests for begin keyword in indexing context (Issue #2310)
# a[begin] should resolve to a[firstindex(a)]

using Test

a = [10, 20, 30, 40, 50]

@testset "begin keyword indexing (Issue #2310)" begin
    # Simple begin indexing
    @test a[begin] == 10

    # begin with arithmetic
    @test a[begin + 1] == 20
    @test a[begin + 2] == 30

    # begin:end range
    @test a[begin:end] == [10, 20, 30, 40, 50]

    # begin+1:end-1 range
    @test a[begin+1:end-1] == [20, 30, 40]

    # Combining begin and end
    @test a[begin] == a[1]
    @test a[end] == a[5]
end

# Additional tests for begin/end symmetry (Issue #2325)
# Note: 2D array begin/end indexing with per-dimension resolution is not yet
# supported. The current implementation uses lastindex(array) without dimension
# awareness. See Issue #2349 for tracking this enhancement.

@testset "String begin indexing" begin
    s = "hello"
    @test s[begin] == 'h'
    @test s[end] == 'o'
    @test s[begin+1] == 'e'
    @test s[end-1] == 'l'
end

@testset "Nested array begin indexing" begin
    nested = [[1, 2], [3, 4]]
    x = nested[begin]
    @test lastindex(x) == 2
    @test nested[begin][begin] == 1
    @test nested[begin][end] == 2
    @test nested[end][begin] == 3
    @test nested[end][end] == 4
end

@testset "begin in comprehension indexing" begin
    arr = [10, 20, 30]
    result = [arr[begin+i] for i in 0:2]
    @test result == [10, 20, 30]
end
end # module Agg_begin_indexing

# ===== source: array/deleteat_multiindex_copy_collect_5744.jl =====
module Agg_deleteat_multiindex_copy_collect_5744
using Test

# Issue #5744: deleteat!(arr, inds) with a vector/range of indices failed for a
# `copy`/`collect` result (a Memory-backed Array-wrapper StructRef) — only the
# native-array literal/var case worked. The compiled `ArrayDeleteAtIndices` fast
# path now falls back, for a StructRef target, to the pure-Julia
# `deleteat!(a::Array, inds)` (untyped `inds` so the #4189 native-array matcher
# guard does not block selection), mirroring the scalar fallback (#5721).

@testset "deleteat! multi-index on copy/collect result (Issue #5744)" begin
    a = copy([1, 2, 3, 4, 5])
    deleteat!(a, [2, 4])
    @test a == [1, 3, 5]

    b = collect(1:5)
    deleteat!(b, 2:3)
    @test b == [1, 4, 5]

    c = copy([10, 20, 30, 40, 50])
    deleteat!(c, 2:4)
    @test c == [10, 50]

    # deleteat! returns the array
    f = copy([1, 2, 3])
    @test deleteat!(f, [1, 3]) == [2]

    # Controls: literal/var multi-index and scalar still work
    @test deleteat!([1, 2, 3, 4, 5], [2, 4]) == [1, 3, 5]
    g = [1, 2, 3]
    deleteat!(g, 2)
    @test g == [1, 3]
    h = collect(1:4)
    deleteat!(h, [1, 2])
    @test h == [3, 4]
end
end # module Agg_deleteat_multiindex_copy_collect_5744

# ===== source: array/fancy_vector_index_5756.jl =====
module Agg_fancy_vector_index_5756
using Test

# Issue #5756: indexing with a vector literal — arr[[1,3,5]] (fancy / vector
# indexing) — was lowered as multi-dimensional arr[1,3,5] and raised a dimension
# mismatch. A vector literal index is a single (fancy) index.

@testset "fancy vector-literal indexing (Issue #5756)" begin
    # 1D fancy indexing with a vector literal
    @test [10, 20, 30, 40, 50][[1, 3, 5]] == [10, 30, 50]
    @test [10, 20, 30, 40, 50][[2, 4]] == [20, 40]
    a = [1, 2, 3, 4, 5]
    @test a[[1, 5]] == [1, 5]
    @test a[[3]] == [3]

    # Order / repetition is preserved
    @test [10, 20, 30][[3, 1, 2]] == [30, 10, 20]
    @test [10, 20, 30][[1, 1, 2]] == [10, 10, 20]

    # Logical (Bool vector literal) indexing
    @test [10, 20, 30][[true, false, true]] == [10, 30]

    # 2D fancy indexing with vector literals on each dimension
    m = [1 2 3; 4 5 6]
    @test m[[1, 2], [1, 3]] == [1 3; 4 6]

    # A vector *variable* index already worked — keep it consistent
    k = [1, 3, 5]
    @test [10, 20, 30, 40, 50][k] == [10, 30, 50]

    # Genuine multi-dimensional indexing on a 1D array is still an error
    @test_throws Exception [1, 2, 3, 4][1, 3]
end
end # module Agg_fancy_vector_index_5756

# ===== source: array/indexed_tuple_assignment_8872.jl =====
module Agg_indexed_tuple_assignment_8872
using Test

data = [1, 2]
data[1], data[2] = data[2], data[1]

@test data == [2, 1]

data[1], x = 9, data[2]

@test data == [9, 1]
@test x == 1
end # module Agg_indexed_tuple_assignment_8872

# ===== source: array/indexin_union_result_4657.jl =====
module Agg_indexin_union_result_4657
using Test

function check_indexin_union_result(a, b, expected)
    r = indexin(a, b)
    ok = typeof(r) === Vector{Union{Nothing, Int64}}
    ok = ok && eltype(r) === Union{Nothing, Int64}
    ok = ok && length(r) == length(expected)
    for i in 1:length(expected)
        if expected[i] === nothing
            ok = ok && r[i] === nothing
        else
            ok = ok && r[i] == expected[i]
        end
    end
    ok
end

@testset "indexin Union{Nothing, Int64} result type (Issues #4018/#4657)" begin
    @test check_indexin_union_result([1, 3], [1, 2], Any[1, nothing])
    @test check_indexin_union_result(Int8[1, 3], Int8[1, 2], Any[1, nothing])
    @test check_indexin_union_result(String["a", "c"], String["a", "b"], Any[1, nothing])
    @test check_indexin_union_result(Any["a", 1], Any[1, "a"], Any[2, 1])
end
end # module Agg_indexin_union_result_4657

# ===== source: array/partialsortperm_sortslices.jl =====
module Agg_partialsortperm_sortslices
# Test partialsortperm, partialsortperm!, sortslices

using Test

@testset "partialsortperm basic (Issue #5745)" begin
    arr = [3.0, 1.0, 4.0, 1.5, 2.0]
    # Integer k returns a single index (the k-th order statistic), not the
    # whole permutation.
    @test partialsortperm(arr, 1) == 2   # 1.0 at index 2
    @test partialsortperm(arr, 3) == 5   # 2.0 at index 5 (3rd smallest)
    @test partialsortperm(arr, 1) isa Integer

    # A range k returns the vector of indices for those order statistics.
    @test partialsortperm(arr, 1:3) == [2, 4, 5]   # 1.0, 1.5, 2.0
    @test partialsortperm(arr, 2:4) == [4, 5, 1]   # 1.5, 2.0, 3.0
end

@testset "partialsortperm! in-place" begin
    arr = [5.0, 2.0, 8.0, 1.0]
    perm = collect(1:4)
    partialsortperm!(perm, arr, 2)
    @test perm[1] == 4  # 1.0
    @test perm[2] == 2  # 2.0
end

@testset "sortslices dims=1 (sort rows)" begin
    A = [3.0 1.0; 1.0 2.0; 2.0 3.0]
    S = sortslices(A; dims=1)
    # Rows sorted lexicographically: [1,2], [2,3], [3,1]
    @test S[1, 1] == 1.0
    @test S[1, 2] == 2.0
    @test S[2, 1] == 2.0
    @test S[2, 2] == 3.0
    @test S[3, 1] == 3.0
    @test S[3, 2] == 1.0
end

@testset "sortslices dims=2 (sort columns)" begin
    A = [3.0 1.0 2.0; 4.0 2.0 3.0]
    S = sortslices(A; dims=2)
    # Columns sorted lexicographically: [1,2], [2,3], [3,4]
    @test S[1, 1] == 1.0
    @test S[2, 1] == 2.0
    @test S[1, 2] == 2.0
    @test S[2, 2] == 3.0
    @test S[1, 3] == 3.0
    @test S[2, 3] == 4.0
end
end # module Agg_partialsortperm_sortslices

true
