using Test
using Plots

function _plots_fixture_aspect_ratio(p)
    if hasproperty(p, :aspect_ratio)
        return p.aspect_ratio
    end
    return p.subplots[1][:aspect_ratio]
end

@testset "Plots: aspect_ratio keyword (Issue #6353)" begin
    p = plot(sin, aspect_ratio=:equal)
    @test _plots_fixture_aspect_ratio(p) === :equal
    if hasproperty(p, :series)
        @test length(p.series) == 1
        @test length(p.series[1].x) == 100
    end

    pn = plot([1, 2, 3], [1.0, 4.0, 9.0], aspect_ratio=2)
    @test _plots_fixture_aspect_ratio(pn) == 2

    pa = scatter([1, 2], [3.0, 4.0], aspectratio=:equal)
    @test _plots_fixture_aspect_ratio(pa) === :equal

    pr = histogram([1, 2, 2], bins=1:3, ratio=:equal)
    @test _plots_fixture_aspect_ratio(pr) === :equal

    p3 = plot([0.0, 1.0], [0.0, 1.0], [0.0, 2.0], axis_ratio=:equal)
    @test _plots_fixture_aspect_ratio(p3) === :equal

    pbase = plot([1, 2], [1.0, 2.0], aspect_ratio=:equal)
    padd = plot!([1, 2], [2.0, 3.0])
    @test _plots_fixture_aspect_ratio(pbase) === :equal
    @test _plots_fixture_aspect_ratio(padd) === :equal
    if hasproperty(padd, :series)
        @test length(padd.series) == 2
    end
end

true
