# Issue #5425: an *unannotated* optional keyword argument with a *typed
# non-`nothing`* default (e.g. `n = 0` → Int64, `s = "x"` → String) was
# inferred/typed from its default literal, so the single compiled body rejected
# a caller-supplied value of a different type:
#
#   function g(; n = 0); n; end
#   g(n = 1.5)   # ERROR: ReturnI64: expected integer, got F64(1.5)
#                # Upstream Julia: 1.5
#
# An unannotated kwarg accepts any value, so its type must be `Any` for the
# no-JIT VM's single compiled body — the default's type must not be used. This
# generalizes the `nothing`-default fix (#5416) to ANY typed default. The
# omitted-kwarg path still keeps the default (`g() == 0`).
#
# Verified against upstream Julia 1.12.

using Test

function kw_typed_default_int_5425(; n = 0)
    return n
end

function kw_typed_default_str_5425(; s = "x")
    return s
end

function kw_typed_default_bool_5425(; flag = false)
    return flag
end

@testset "unannotated kwarg with typed default accepts any value (#5425)" begin
    @testset "Int64 default accepts a Float64 value" begin
        @test kw_typed_default_int_5425(n = 1.5) == 1.5
        @test typeof(kw_typed_default_int_5425(n = 1.5)) === Float64
        @test kw_typed_default_int_5425(n = "hi") == "hi"
        @test kw_typed_default_int_5425(n = [1, 2]) == [1, 2]
    end

    @testset "Int64 default kept when omitted" begin
        @test kw_typed_default_int_5425() == 0
        @test typeof(kw_typed_default_int_5425()) === Int64
        @test kw_typed_default_int_5425(n = 7) == 7
        @test typeof(kw_typed_default_int_5425(n = 7)) === Int64
    end

    @testset "String default accepts an Int64 value" begin
        @test kw_typed_default_str_5425(s = 42) == 42
        @test typeof(kw_typed_default_str_5425(s = 42)) === Int64
        @test kw_typed_default_str_5425() == "x"
        @test typeof(kw_typed_default_str_5425()) === String
    end

    @testset "Bool default accepts an Int64 value" begin
        @test kw_typed_default_bool_5425(flag = 3) == 3
        @test typeof(kw_typed_default_bool_5425(flag = 3)) === Int64
        @test kw_typed_default_bool_5425() === false
    end

    @testset "omitted-kwarg signature keeps the default type for reflection" begin
        # The compiled body is `Any`, but the per-call-signature reflection for
        # the omitted-kwarg call must stay precise (matches upstream).
        @test Base.infer_return_type(kw_typed_default_int_5425, Tuple{}) === Int64
        @test Base.infer_return_type(kw_typed_default_str_5425, Tuple{}) === String
    end
end

true
