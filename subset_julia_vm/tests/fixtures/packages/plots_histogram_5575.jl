using Test
using Plots

@testset "Plots: histogram and histogram! (Issue #5575)" begin
    data = [1, 2, 1, 1, 4, 3, 8]
    p = histogram(data, bins=0:8)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :bar
    @test s.x == [0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5]
    @test s.y == [0.0, 3.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0]

    wrapped = histogram(data, bins=0:8, weights=weights([4, 7, 3, 9, 12, 2, 6]))
    @test wrapped.series[1].seriestype === :bar
    @test wrapped.series[1].y == [0.0, 16.0, 7.0, 2.0, 12.0, 0.0, 0.0, 6.0]

    raw_weights = [4, 7, 3, 9, 12, 2, 6]
    pw = histogram(data, bins=0:8, weights=raw_weights)
    @test pw.series[1].y == [0.0, 16.0, 7.0, 2.0, 12.0, 0.0, 0.0, 6.0]

    pp = histogram(data, bins=0:8, normalize=:probability)
    @test isapprox(sum(pp.series[1].y), 1.0)
    @test isapprox(pp.series[1].y[2], 3.0 / 7.0)

    ppdf = histogram(data, bins=0:8, normalize=true)
    @test isapprox(sum(ppdf.series[1].y), 1.0)

    p_int = histogram([1, 2, 3, 4, 5], bins=2)
    @test p_int.series[1].seriestype === :bar
    @test p_int.series[1].x == [2.0, 4.0]
    @test p_int.series[1].y == [2.0, 3.0]

    p2 = histogram([1, 2], bins=1:3)
    p3 = histogram!([2, 2, 3], bins=1:4)
    @test length(p2.series) == 2
    @test length(p3.series) == 2
    @test p3.series[2].seriestype === :bar
    @test p3.series[2].x == [1.5, 2.5, 3.5]
    @test p3.series[2].y == [0.0, 2.0, 1.0]
end

true
