using Test
using Plots

# Issue #6355: the combined `@gif for ... end` form builds the animation and
# immediately wraps it in an `AnimatedGif` (one frame per iteration).
@testset "Plots: @gif builds an AnimatedGif directly" begin
    p = plot(1)
    g = @gif for x = 0:0.1:5
        push!(p, 1, sin(x))
    end

    @test isa(g, AnimatedGif)
    @test length(g.frames) == 51
end

true
