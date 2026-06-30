using Test
using Plots

# Issue #6355: `@animate` collects one Plot snapshot per loop iteration into an
# `Animation`. `0:0.1:5` has 51 elements, so we expect 51 frames. The final frame
# captures the accumulated series: 1 initial point from `plot(1)` plus 51 pushes.
@testset "Plots: @animate collects one frame per iteration" begin
    p = plot(1)
    anim = @animate for x = 0:0.1:5
        push!(p, 1, sin(x))
    end

    @test isa(anim, Animation)
    @test length(anim.frames) == 51

    first_frame = anim.frames[1]
    @test length(first_frame.series[1].y) == 2

    last_frame = anim.frames[length(anim.frames)]
    @test length(last_frame.series[1].y) == 52
end

true
