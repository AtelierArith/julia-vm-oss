using Test
using Plots

# Issue #7272: `@animate for ... end every N` captures a frame every N iterations;
# `when cond` captures only when `cond` is true. The trailing `every N` / `when c`
# are extra macro arguments parsed after the loop body, interpreted as a frame
# filter (upstream: `mod1(counter, N) == 1`, counter starting at 1).
@testset "Plots: @animate ... every N samples one frame per N iterations" begin
    anim = @animate for i in 1:30
        plot([0.0, Float64(i)], [0.0, Float64(i)])
    end every 10

    @test isa(anim, Animation)
    # counter 1..30, captured when mod1(counter, 10) == 1 -> 1, 11, 21 -> 3 frames.
    @test length(anim.frames) == 3

    g = gif(anim)
    @test isa(g, AnimatedGif)
    @test length(g.frames) == 3
end

@testset "Plots: @animate ... when cond captures only on true" begin
    anim = @animate for i in 1:10
        plot([0.0, Float64(i)], [0.0, Float64(i)])
    end when i % 3 == 0

    @test isa(anim, Animation)
    # i in 3, 6, 9 -> 3 frames.
    @test length(anim.frames) == 3
end

@testset "Plots: @gif ... every N wraps the sampled frames" begin
    g = @gif for i in 1:30
        plot([0.0, Float64(i)], [0.0, Float64(i)])
    end every 10

    @test isa(g, AnimatedGif)
    @test length(g.frames) == 3
end

@testset "Plots: bare @animate (no modifier) still captures every frame" begin
    anim = @animate for i in 1:5
        plot([0.0, Float64(i)], [0.0, Float64(i)])
    end
    @test length(anim.frames) == 5
end

true
