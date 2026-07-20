# Test: struct implementing the Base iterate protocol propagates its
# element type into `for x in value`, instead of degrading the loop variable
# (and everything derived from it) to `unknown`/dynamic dispatch (Issue #9124).

using Test

struct Point9124
    x::Float64
    y::Float64
end

struct PointList9124
    pts::Vector{Point9124}
end

function Base.iterate(pl::PointList9124, state=1)
    state > length(pl.pts) && return nothing
    return pl.pts[state], state + 1
end

function sum_x_9124(pl::PointList9124)
    s = 0.0
    for p in pl
        s += p.x
    end
    return s
end

@testset "struct iterate protocol propagates element type (Issue #9124)" begin
    pl = PointList9124([Point9124(1.0, 2.0), Point9124(3.0, 4.0)])
    @test sum_x_9124(pl) == 4.0

    # Empty collection: zero iterations, no element-type-dependent code runs.
    @test sum_x_9124(PointList9124(Point9124[])) == 0.0

    # Comprehension-built Vector{Point9124} field: the source array is
    # `ArrayOf(Any)`-tagged before conversion into the declared
    # `Vector{Point9124}` field, the exact case the fix's engine-only lattice
    # enrichment must not miscompile (Issue #9124 prior-analysis blocker).
    pl2 = PointList9124([Point9124(Float64(i), Float64(i)) for i in 1:4])
    @test sum_x_9124(pl2) == 10.0
end

# An explicitly-typed state parameter must dispatch/re-infer the same way.
struct TypedStateList9124
    pts::Vector{Point9124}
end

function Base.iterate(pl::TypedStateList9124, state::Int64=1)
    state > length(pl.pts) && return nothing
    return pl.pts[state], state + 1
end

function sum_y_9124(pl::TypedStateList9124)
    s = 0.0
    for p in pl
        s += p.y
    end
    return s
end

@testset "struct iterate with explicitly typed state param (Issue #9124)" begin
    pl = TypedStateList9124([Point9124(1.0, 2.0), Point9124(3.0, 4.0), Point9124(5.0, 6.0)])
    @test sum_y_9124(pl) == 12.0
end

# Builtin iterate paths (Array, Range, Dict) must remain unaffected.
@testset "builtin iterate protocols unaffected (Issue #9124)" begin
    total = 0
    for i in 1:5
        total += i
    end
    @test total == 15

    arr_total = 0.0
    for x in [1.0, 2.0, 3.0]
        arr_total += x
    end
    @test arr_total == 6.0

    d = Dict("a" => 1, "b" => 2)
    kv_total = 0
    for (k, v) in d
        kv_total += v
    end
    @test kv_total == 3
end

true
