# Issue #9421: float colon ranges materialize with TwicePrecision semantics.
# collect(0:0.1:1) must yield the shortest-decimal grid values (0.3, not the
# naive accumulation 0.30000000000000004), matching upstream StepRangeLen
# getindex (julia/base/twiceprecision.jl unsafe_getindex).

using Test

@testset "collect(0:0.1:1) is TwicePrecision-exact (Issue #9421)" begin
    c = collect(0:0.1:1)
    @test length(c) == 11
    @test c == [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
    # The failing elements under naive ref + i*step accumulation:
    @test c[4] === 0.3
    @test c[7] === 0.6
    @test c[8] === 0.7
end

@testset "getindex / last / iteration match collect (Issue #9421)" begin
    r = 0:0.1:1
    @test r[4] === 0.3
    @test last(r) === 1.0
    seen = Float64[]
    for x in r
        push!(seen, x)
    end
    @test seen == collect(r)
    # Grid membership is exact upstream: 0.3 in 0:0.1:1 is true.
    @test (0.3 in r)
    @test !(0.35 in r)
end

@testset "descending and offset float ranges (Issue #9421)" begin
    @test collect(1.0:-0.1:0.5) == [1.0, 0.9, 0.8, 0.7, 0.6, 0.5]
    @test collect(-0.3:0.1:0.3) == [-0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3]
    @test collect(0.1:0.1:0.3) == [0.1, 0.2, 0.3]
    # Slicing a float range keeps the exact grid values.
    @test collect((0:0.1:1)[3:5]) == [0.2, 0.3, 0.4]
    # Non-rational steps keep the literal (start + k*step) values.
    # (1.0 * pi keeps the expected vector Float64-typed — sjulia array
    # literals do not yet promote raw Irrational elements, Issue #9511.)
    r = 0.0:pi:10.0
    @test collect(r) == [0.0, 1.0 * pi, 2.0 * pi, 3.0 * pi]
end

true
