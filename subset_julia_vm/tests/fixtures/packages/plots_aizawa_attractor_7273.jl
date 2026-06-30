using Test
using Plots

# Issue #7273: Aizawa attractor 3D animation iOS sample. Exercises the Plots
# building blocks merged for the Lorenz family — Base.@kwdef mutable struct,
# plot3d(1) (empty 3D path), push!(plt, x, y, z) (Issue #7271), and
# @animate ... every N (Issue #7272) — composed into one strange-attractor demo.
#
# NOTE: the sjulia Plots subset exposes Animation/AnimatedGif as bare exports,
# a no-filename gif(anim), an AnimatedGif.frames field, and a Plot.series field.
# Upstream Plots.jl differs (Plots.Animation/Plots.AnimatedGif, gif needs a path,
# AnimatedGif has only .filename, Plot uses .series_list). The dynamics plus
# `isa(anim, Animation)` and `length(anim.frames) == 150` are upstream-portable.

Base.@kwdef mutable struct Aizawa
    dt::Float64 = 0.01
    a::Float64 = 0.95
    b::Float64 = 0.7
    c::Float64 = 0.6
    d::Float64 = 3.5
    e::Float64 = 0.25
    f::Float64 = 0.1
    x::Float64 = 0.1
    y::Float64 = 0.0
    z::Float64 = 0.0
end

function step!(s::Aizawa)
    dx = (s.z - s.b) * s.x - s.d * s.y
    dy = s.d * s.x + (s.z - s.b) * s.y
    dz = s.c + s.a * s.z - s.z^3 / 3 - (s.x^2 + s.y^2) * (1 + s.e * s.z) + s.f * s.z * s.x^3
    s.x = s.x + s.dt * dx
    s.y = s.y + s.dt * dy
    s.z = s.z + s.dt * dz
    return s
end

@testset "Aizawa: Base.@kwdef supplies the default parameters" begin
    a = Aizawa()
    @test a.x == 0.1
    @test a.y == 0.0
    @test a.z == 0.0
    @test a.dt == 0.01
    @test a.d == 3.5
end

@testset "Aizawa: step! advances the orbit by one Euler step" begin
    a = Aizawa()
    step!(a)
    # x' = x + dt*((z-b)*x - d*y) = 0.1 + 0.01*((0-0.7)*0.1) = 0.0993
    @test a.x == 0.0993
    # y' = y + dt*(d*x + (z-b)*y) = 0 + 0.01*(3.5*0.1) (Float64-exact, matches upstream).
    @test a.y == 0.0035000000000000005
    # z' = z + dt*(c + a*z - z^3/3 - (x^2+y^2)*(1+e*z) + f*z*x^3) = 0.01*(0.6 - 0.01) = 0.0059
    @test a.z == 0.0059
end

@testset "Aizawa: plot3d(1) starts an empty 3D path series" begin
    plt = plot3d(1, xlim=(-1.5,1.5), ylim=(-1.5,1.5), zlim=(-0.5,1.7),
                 title="Aizawa Attractor", legend=false, marker=2)
    @test plt.series[1].seriestype === :path3d
    @test length(plt.series[1].x) == 0
end

@testset "Aizawa: @animate ... every 20 over 3000 steps builds 150 frames" begin
    attractor = Aizawa()
    plt = plot3d(1, xlim=(-1.5,1.5), ylim=(-1.5,1.5), zlim=(-0.5,1.7),
                 title="Aizawa Attractor", legend=false, marker=2)
    anim = @animate for i in 1:3000
        step!(attractor)
        push!(plt, attractor.x, attractor.y, attractor.z)
    end every 20

    @test isa(anim, Animation)
    # counter 1..3000, captured when mod1(counter, 20) == 1 -> 1, 21, ..., 2981 -> 150 frames.
    @test length(anim.frames) == 150
    # every push! appended a point to the single 3D series.
    @test length(plt.series[1].x) == 3000

    g = gif(anim)
    @test isa(g, AnimatedGif)
    @test length(g.frames) == 150
end

true
