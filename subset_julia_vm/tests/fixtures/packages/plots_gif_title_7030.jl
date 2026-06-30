using Test
using Plots

# Issue #7030: `plot(...; title=...)` carries a title, and `@gif`/`@animate` loops that
# build a fresh titled plot each iteration capture a per-frame title. This also
# exercises broadcast (`sin.(...)`), keyword args, and string interpolation round-
# tripping through the `@gif` macro's quote path (Issue #7029) — previously each of
# those raised `quote for ... not yet supported`.
@testset "Plots: @gif per-frame title with broadcast + interpolation" begin
    x = [0.0, 1.0, 2.0]
    g = @gif for t in 1:3
        plot(x, sin.(x .- t), title="t=$t")
    end
    @test isa(g, AnimatedGif)
    @test length(g.frames) == 3
    @test g.frames[1].title == "t=1"
    @test g.frames[2].title == "t=2"
    @test g.frames[3].title == "t=3"
    # broadcast inside the macro body computed the shifted sine for each frame.
    @test g.frames[1].series[1].y == sin.(x .- 1)
    @test g.frames[3].series[1].y == sin.(x .- 3)
end

# `@animate` over the same body collects an Animation with per-frame titles.
@testset "Plots: @animate per-frame title" begin
    anim = @animate for t in 1:2
        plot([1.0, 2.0], [Float64(t), Float64(t)], title="frame $t")
    end
    @test isa(anim, Animation)
    @test length(anim.frames) == 2
    @test anim.frames[1].title == "frame 1"
    @test anim.frames[2].title == "frame 2"
end

# A static titled plot (and shorthands) keep the title field.
@testset "Plots: static title field" begin
    p = plot([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], title="hello")
    @test p.title == "hello"
    @test scatter([1.0, 2.0], [3.0, 4.0], title="sc").title == "sc"
    # No title → empty string (not an error).
    @test plot([1.0, 2.0]).title == ""
end

# Replaying a list of pre-built titled plots with `@gif for p in ps; plot(p); end`
# (#7026 plot(p::Plot) + #7030 title): each frame keeps its plot's title.
@testset "Plots: @gif replays pre-built titled plots" begin
    ps = []
    for t in 1:3
        push!(ps, plot([1.0, 2.0], [Float64(t), Float64(t)], title="t=$t"))
    end
    g = @gif for p in ps
        plot(p)
    end
    @test isa(g, AnimatedGif)
    @test length(g.frames) == 3
    @test g.frames[1].title == "t=1"
    @test g.frames[3].title == "t=3"
end

true
