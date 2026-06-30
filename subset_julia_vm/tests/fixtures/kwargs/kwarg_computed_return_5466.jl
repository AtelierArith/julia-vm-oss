# Issue #5466 (follow-up to #5425 / PR #5464): an *unannotated* optional keyword
# argument with a typed non-`nothing` default whose value is returned *through a
# computation* (not returned directly) was still rejected at runtime:
#
#   function g2(; n = 0); return n + 1; end
#   g2(n = 1.5)   # ERROR: InternalError: ReturnI64: expected integer, got F64(2.5)
#                 # Upstream Julia: 2.5
#
# #5425 widened the body / dispatch / call-site return-type channels only for
# functions that return the kwarg *directly* (`g(; n = 0) = n`). When the body
# returns a value *derived from* such a kwarg (`n + 1`, `2 * n`, or via a local
# binding `m = n + 1; return m`), the kwarg slot was already `Any` (so `n` loads
# dynamically), but the function's inferred return type stayed concrete
# (`Int64`), so the compiled body emitted a typed `ReturnI64` that rejected a
# `Float64` result. This generalizes the fix to "returns a value DERIVED FROM an
# unannotated optional kwparam".
#
# `FunctionInfo.return_type` stays precise, so reflection
# (`Base.infer_return_type(g2, Tuple{})` -> `Int64`) keeps the omitted-kwarg
# signature's type. A genuinely *type-annotated* kwarg (`n::Int = 0`) is NOT an
# unannotated optional kwparam, so it must NOT be widened.
#
# Verified against upstream Julia 1.12.

using Test

function kw_computed_int_5466(; n = 0)
    return n + 1
end

function kw_computed_mul_5466(; n = 0)
    return 2 * n
end

# Multi-statement body: the returned value is derived from the kwarg through a
# local binding, not returned directly.
function kw_computed_local_5466(; n = 0)
    m = n + 1
    return m
end

# A genuinely type-annotated kwarg is excluded from the widening; reflection and
# matching-value behavior must stay precise (over-widening guard).
function kw_annotated_int_5466(; n::Int = 0)
    return n + 1
end

@testset "computed return of an unannotated optional kwarg accepts any value (#5466)" begin
    @testset "Int64 default, body `n + 1`, accepts a Float64" begin
        @test kw_computed_int_5466(n = 1.5) == 2.5
        @test typeof(kw_computed_int_5466(n = 1.5)) === Float64
        @test kw_computed_int_5466() == 1
        @test typeof(kw_computed_int_5466()) === Int64
        @test kw_computed_int_5466(n = 2) == 3
        @test typeof(kw_computed_int_5466(n = 2)) === Int64
    end

    @testset "Int64 default, body `2 * n`, accepts a Float64" begin
        @test kw_computed_mul_5466(n = 1.5) == 3.0
        @test typeof(kw_computed_mul_5466(n = 1.5)) === Float64
        @test kw_computed_mul_5466() == 0
        @test kw_computed_mul_5466(n = 4) == 8
    end

    @testset "derived through a local binding accepts a Float64" begin
        @test kw_computed_local_5466(n = 1.5) == 2.5
        @test typeof(kw_computed_local_5466(n = 1.5)) === Float64
        @test kw_computed_local_5466() == 1
    end

    @testset "computed return stays precise for reflection (omitted-kwarg signature)" begin
        @test Base.infer_return_type(kw_computed_int_5466, Tuple{}) === Int64
        @test Base.infer_return_type(kw_computed_mul_5466, Tuple{}) === Int64
    end

    @testset "annotated kwarg is NOT widened (no over-widening guard)" begin
        @test kw_annotated_int_5466() == 1
        @test typeof(kw_annotated_int_5466()) === Int64
        @test kw_annotated_int_5466(n = 5) == 6
        @test typeof(kw_annotated_int_5466(n = 5)) === Int64
        @test Base.infer_return_type(kw_annotated_int_5466, Tuple{}) === Int64
    end
end

true
