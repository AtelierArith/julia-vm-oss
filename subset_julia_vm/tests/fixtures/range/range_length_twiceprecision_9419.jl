# Issue #9419: range(start, stop; length) on Int64/Float64 endpoints returns
# upstream's TwicePrecision-backed StepRangeLen (not LinRange), with exact
# _linspace element values (julia/base/twiceprecision.jl).

using Test

@testset "range(;length) return type parity (Issue #9419)" begin
    @test string(typeof(range(0, 1, length=3))) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test string(typeof(range(0.0, 1.0, length=3))) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    # Positional-length form routes through the same path.
    @test string(typeof(range(0, 1, 5))) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
end

@testset "range(;length) values are _linspace-exact (Issue #9419)" begin
    r = range(0, 1, length=3)
    @test first(r) === 0.0
    @test last(r) === 1.0
    @test step(r) === 0.5
    @test r[2] === 0.5
    @test length(r) == 3
    @test collect(r) == [0.0, 0.5, 1.0]
    # Non-dyadic step: shortest-decimal grid, endpoints exact.
    @test collect(range(0, 1, length=7)) == [
        0.0,
        0.16666666666666666,
        0.3333333333333333,
        0.5,
        0.6666666666666666,
        0.8333333333333334,
        1.0,
    ]
    @test collect(range(0.1, 0.9, length=5)) == [0.1, 0.3, 0.5, 0.7, 0.9]
    # Integer endpoints promote to Float64 (upstream _linspace(float(T), ...)).
    @test collect(range(1, 5, length=5)) == [1.0, 2.0, 3.0, 4.0, 5.0]
    # Irrational endpoints: first/last land exactly on the inputs.
    r2 = range(0.1, pi, length=5)
    @test first(r2) === 0.1
    @test last(r2) === Float64(pi)
end

@testset "range(;length) degenerate lengths (Issue #9419)" begin
    @test collect(range(1, 1, length=5)) == [1.0, 1.0, 1.0, 1.0, 1.0]
    @test isempty(collect(range(0, 1, length=0)))
    @test collect(range(2.5, stop=2.5, length=1)) == [2.5]
    # show form for a step-0 length-defined range is the constructor form.
    @test string(range(1, 1, length=5)) == "StepRangeLen(1.0, 0.0, 5)"
    @test string(range(0, 1, length=3)) == "0.0:0.5:1.0"
end

true
