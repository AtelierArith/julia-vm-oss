# Tuple / NamedTuple element-type sharpening (Issue #5183)
#
# The codegen typer recovers the precise element type of a statically known
# tuple / NamedTuple at a constant index (or destructuring position), instead of
# collapsing it to `Any`. The observable runtime results below are identical to
# upstream Julia; sharpening keeps these uses type-stable for dispatch.
#
# Single flat @testset so the pass/fail summary matches upstream's aggregate
# line for scripts/fixture_julia_parity.sh.

using Test

# Multi-value returning helper: Tuple{Int64, Float64}
function make_pair()
    return (3, 4.0)
end

# NamedTuple-returning helper: @NamedTuple{a::Int64, b::Float64}
function make_named()
    return (a = 5, b = 6.0)
end

@testset "Tuple element sharpening (Issue #5183)" begin
    # constant index over a tuple-typed local
    t = make_pair()
    @test t[1] == 3
    @test t[2] == 4.0
    @test typeof(t[1]) === Int64
    @test typeof(t[2]) === Float64
    @test t[1] + 1 == 4
    @test t[2] + 1 == 5.0
    @test t[1] / 2 == 1.5

    # destructuring keeps each binding typed
    (a, b) = make_pair()
    @test typeof(a) === Int64
    @test typeof(b) === Float64
    @test a + 1 == 4
    @test b + 1 == 5.0
    @test a / 2 == 1.5
    @test b / 2 == 2.0

    # first / last on a tuple-typed local
    @test first(t) == 3
    @test last(t) == 4.0
    @test typeof(first(t)) === Int64
    @test typeof(last(t)) === Float64

    # tuple literal bound to a local
    v = (10, 20, 30)
    @test v[1] == 10
    @test v[3] == 30
    @test v[1] + v[3] == 40
    @test typeof(v[2]) === Int64

    # named tuple constant index and field access
    nt = make_named()
    @test nt[1] == 5
    @test nt[2] == 6.0
    @test typeof(nt[1]) === Int64
    @test typeof(nt[2]) === Float64
    @test nt.a + 1 == 6
    @test nt.b + 1 == 7.0

    # string element stays correct
    s = ("hello", 42)
    @test s[1] == "hello"
    @test s[1] * "!" == "hello!"
    @test s[2] + 1 == 43

    # dynamic (non-constant) index still works
    total = 0
    for i in 1:3
        total += v[i]
    end
    @test total == 60
end

true
