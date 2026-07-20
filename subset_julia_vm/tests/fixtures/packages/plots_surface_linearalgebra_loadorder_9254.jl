using Test
using Plots
using LinearAlgebra   # loaded AFTER Plots: the #9254 load order that used to defeat dispatch

# Issue #9254: `surface(x, y, zf::Function)` must sample the function over the
# x×y grid and build a 3D `:surface` Series REGARDLESS of whether/when an
# unrelated stdlib (`LinearAlgebra`) is imported. The preloaded-package bytecode
# cache (#9189/#9245) used to splice frozen call-target indices that a lifted
# main lambda shifted, so `surface(x, y, (x,y) -> …)` silently degraded to a 2D
# `:path` line once `using LinearAlgebra` followed `using Plots`.

@testset "Plots #9254: surface(x,y,zf::Function) after using LinearAlgebra" begin
    xs = [10.0, 20.0]
    ys = [1.0, 2.0, 3.0]
    p = surface(xs, ys, (x, y) -> y * 100 + x)
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :surface       # NOT :path / :scatter (the bug)
    @test length(s.x) == 2
    @test length(s.y) == 3
    # z orientation: size(z) == (length(ys), length(xs)) i.e. row=y, col=x.
    @test s.z[1, 1] == 110.0
    @test s.z[2, 1] == 210.0
    @test s.z[3, 2] == 320.0
end

@testset "Plots #9254: iOS Sinc-Surface sample shape (norm inside the lambda)" begin
    x = y = range(-3, stop = 3, length = 5)
    p = surface(x, y, (x, y) -> sinc(norm([x, y])))
    @test length(p.series) == 1
    s = p.series[1]
    @test s.seriestype === :surface
    @test length(s.x) == 5
    @test length(s.y) == 5
    # sinc(norm([0,0])) == sinc(0) == 1 at the grid centre (row 3, col 3).
    @test s.z[3, 3] ≈ 1.0
end

true
