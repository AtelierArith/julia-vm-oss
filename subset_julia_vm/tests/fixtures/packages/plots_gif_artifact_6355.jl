using Test
using Plots

# Issue #6355: `gif(anim)` wraps the accumulated frames into an `AnimatedGif`,
# the value the Rust artifact pipeline turns into a Plotly frames animation.
@testset "Plots: gif(anim) returns an AnimatedGif" begin
    p = plot(1)
    anim = @animate for x = 0:0.1:5
        push!(p, 1, sin(x))
    end

    g = gif(anim)
    @test isa(g, AnimatedGif)
    @test length(g.frames) == 51
    @test g.fps == 20

    g2 = gif(anim; fps = 30)
    @test g2.fps == 30
end

true
