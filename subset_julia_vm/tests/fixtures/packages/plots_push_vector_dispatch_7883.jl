using Test
using Plots

# Issue #7883: once `using Plots` brings a same-arity `push!(::Plot, ::Number)`
# method into scope, the compiler routes EVERY `push!(v, x)` (even on an ordinary
# `Vector`) through `CallTypedDispatchOrBuiltin`. The `BuiltinId::Push` builtin
# fallback used to snapshot + reallocate the whole backing `Memory` on every call
# (O(n) per push, O(n^2) for a loop); it now grows the wrapper in place via the
# same `push_array_wrapper` path the native `ArrayPush` instruction uses (Issue
# #6873). This fixture pins the CORRECTNESS of that dispatch-routed fast path at a
# non-trivial scale — values must be exactly those pushed, in order, with no
# corruption or truncation from the in-place growth.

@testset "push!(::Vector{Float64}) through the Plots-poisoned dispatch path is correct at scale" begin
    n = 8000
    v = Float64[]
    for i in 1:n
        push!(v, Float64(i) * 0.5)
    end
    @test length(v) == n
    @test v[1] == 0.5
    @test v[2] == 1.0
    @test v[n] == Float64(n) * 0.5
    @test v[n - 1] == Float64(n - 1) * 0.5
    @test sum(v) == 0.5 * (n * (n + 1) / 2)
end

@testset "push!(::Vector{Any}) keeps heterogeneous values verbatim through dispatch" begin
    w = Any[]
    push!(w, 1)
    push!(w, "x")
    push!(w, 3.0)
    push!(w, :sym)
    @test length(w) == 4
    @test w[1] === 1
    @test w[2] == "x"
    @test w[3] === 3.0
    @test w[4] === :sym
end

@testset "push!(plt, x, y, z) on a 3D path series grows the series correctly at scale" begin
    plt = plot3d(1, legend=false)
    n = 8000
    for i in 1:n
        push!(plt, Float64(i), Float64(2i), Float64(3i))
    end
    s = plt.series[1]
    @test length(s.x) == n
    @test length(s.y) == n
    @test length(s.z) == n
    @test s.x[1] == 1.0
    @test s.y[1] == 2.0
    @test s.z[1] == 3.0
    @test s.x[n] == Float64(n)
    @test s.y[n] == Float64(2n)
    @test s.z[n] == Float64(3n)
end

true
