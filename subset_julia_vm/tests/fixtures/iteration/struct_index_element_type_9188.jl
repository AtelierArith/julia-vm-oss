# Test: a struct value loaded from an array element via `arr[i]` (IndexLoad)
# propagates its concrete element type into the destination local slot, so
# follow-on `.field` access specializes to typed `GetField` + native ops
# instead of degrading to dynamic `GetFieldByName` dispatch (Issue #9188).
#
# Distinct from #9124 (the `iterate`-protocol element type): this covers the
# array-index (`getindex`/`IndexLoad`) -> field-access path for a struct field
# declared `Vector{T}`/`Matrix{T}`.

using Test

struct Pt9188
    x::Float64
    y::Float64
    z::Float64
end

struct Container9188
    items::Vector{Pt9188}
end

function sink_9188(c::Container9188)
    s = 0.0
    for i in eachindex(c.items)
        p = c.items[i]
        s += p.x
    end
    return s
end

# Direct chained access with no intermediate local.
function sink_chained_9188(c::Container9188)
    s = 0.0
    for i in eachindex(c.items)
        s += c.items[i].x
    end
    return s
end

@testset "array-index element type propagates into local slot (Issue #9188)" begin
    c = Container9188([Pt9188(1.0, 2.0, 3.0), Pt9188(4.0, 5.0, 6.0)])
    @test sink_9188(c) == 5.0
    @test sink_chained_9188(c) == 5.0

    # Empty collection: zero iterations, no element-type-dependent code runs.
    @test sink_9188(Container9188(Pt9188[])) == 0.0

    # Comprehension-built Vector{Pt9188} field: the source array is
    # `ArrayOf(Any)`-tagged before conversion into the declared
    # `Vector{Pt9188}` field — the exact ArrayOf-convert regression risk
    # documented by the prior #9124 analysis; the fix must not miscompile it.
    c2 = Container9188([Pt9188(Float64(i), Float64(i) * 2, Float64(i) * 3) for i in 1:3])
    @test sink_9188(c2) == 6.0
    @test c2.items[2].y == 4.0
end

# `Vector{T}`/`Matrix{T}` parameter annotations (Issue #9133) must keep
# working after routing the struct-element case through the same helper.
function first_x_9188(v::Vector{Pt9188})
    return v[1].x
end

struct Grid9188
    cells::Matrix{Pt9188}
end

@testset "Vector{Struct}/Matrix{Struct} parameter and field annotations (Issue #9188)" begin
    items = [Pt9188(1.0, 0.0, 0.0), Pt9188(2.0, 0.0, 0.0)]
    @test first_x_9188(items) == 1.0

    g = Grid9188(reshape([Pt9188(Float64(i), 0.0, 0.0) for i in 1:4], 2, 2))
    @test g.cells[1, 2].x == 3.0
end

# `push!` into a `Vector{Struct}` field (the Aizawa-sample hot-loop shape)
# must still build and index correctly.
mutable struct MutableContainer9188
    items::Vector{Pt9188}
end

struct NumContainer9188
    vals::Vector{Float64}
end

@testset "push! into Vector{Struct} field (Issue #9188)" begin
    mc = MutableContainer9188(Pt9188[])
    for i in 1:3
        push!(mc.items, Pt9188(Float64(i), 0.0, 0.0))
    end
    s = 0.0
    for i in eachindex(mc.items)
        s += mc.items[i].x
    end
    @test s == 6.0
end

# Builtin numeric/Array/Range/Tuple/Dict indexing must remain unaffected.
@testset "builtin indexing unaffected (Issue #9188)" begin
    a = [1, 2, 3]
    @test a[2] == 2

    b = Float64[1.0, 2.0, 3.0]
    @test b[3] == 3.0

    r = 1:10
    @test r[3] == 3
    @test a[1:2] == [1, 2]

    d = Dict("a" => 1, "b" => 2)
    @test d["a"] == 1

    t = (1, "x", 3.0)
    @test t[2] == "x"

    nc = NumContainer9188([1.0, 2.0, 3.0])
    s = 0.0
    for i in eachindex(nc.vals)
        s += nc.vals[i]
    end
    @test s == 6.0
end

true
