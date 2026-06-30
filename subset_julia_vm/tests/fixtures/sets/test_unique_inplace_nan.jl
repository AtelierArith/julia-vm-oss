using Test

# Regression test for Issue #3585:
# `unique!([NaN, NaN])` must collapse duplicate `NaN` values to a single
# element, matching official Julia. Julia's set-like uniqueness APIs
# (including the in-place `unique!`) use `isequal` semantics so that
# `NaN` equals `NaN` for uniqueness purposes (even though `NaN == NaN`
# is `false` arithmetically).
#
# The implementation in `subset_julia_vm/src/julia/base/set.jl` uses
# `isequal` at the duplicate-detection site; this fixture pins the
# behavior so a future regression to `==` is caught immediately.

@testset "unique! with NaN (#3585)" begin
    # MWE from the issue: two NaNs collapse to one.
    x1 = [NaN, NaN]
    unique!(x1)
    @test length(x1) == 1
    @test isnan(x1[1])

    # Three NaNs collapse to one.
    x2 = [NaN, NaN, NaN]
    unique!(x2)
    @test length(x2) == 1
    @test isnan(x2[1])

    # NaN interleaved with finite values: order preserved, single NaN kept.
    x3 = [1.0, NaN, NaN, 2.0]
    unique!(x3)
    @test length(x3) == 3
    @test x3[1] == 1.0
    @test isnan(x3[2])
    @test x3[3] == 2.0

    # NaN at the end after dedup of finite values.
    x4 = [1.0, 1.0, NaN, NaN]
    unique!(x4)
    @test length(x4) == 2
    @test x4[1] == 1.0
    @test isnan(x4[2])

    # Ordinary integer dedup still works (regression guard).
    x5 = [1, 2, 1]
    unique!(x5)
    @test length(x5) == 2
    @test x5[1] == 1
    @test x5[2] == 2

    # unique!([1, 2, 1]) leaves [1, 2] in-place per acceptance criteria.
    x6 = [1, 2, 1]
    result6 = unique!(x6)
    @test result6 === x6
    @test x6 == [1, 2]
end

true
