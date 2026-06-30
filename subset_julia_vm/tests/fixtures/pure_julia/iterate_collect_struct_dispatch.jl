# Test that iterate / collect dispatch through Pure Julia method tables
# (Issue #3735). When a user-defined struct provides `iterate` and
# `collect` methods, normal dispatch must select them instead of the
# Rust builtin `BuiltinId::Iterate` / `BuiltinId::RangeCollect`.

using Test

struct Counter
    n::Int64
end

# Pure Julia iterate methods on a user struct.
function iterate(c::Counter)
    if c.n <= 0
        return nothing
    end
    return (1, 1)
end

function iterate(c::Counter, state::Int64)
    if state >= c.n
        return nothing
    end
    nxt = state + 1
    return (nxt, nxt)
end

# Pure Julia collect specialization on the user struct.
function collect(c::Counter)
    out = Int64[]
    s = iterate(c)
    while s !== nothing
        v, st = s
        push!(out, v + 1000)
        s = iterate(c, st)
    end
    return out
end

@testset "iterate dispatch on user-defined struct (Issue #3735)" begin
    c = Counter(3)
    s = iterate(c)
    @test (s !== nothing)
    @test (s[1] == 1)
    @test (s[2] == 1)

    s2 = iterate(c, 1)
    @test (s2 !== nothing)
    @test (s2[1] == 2)
    @test (s2[2] == 2)

    s3 = iterate(c, 3)
    @test (s3 === nothing)
end

@testset "collect dispatch on user-defined struct (Issue #3735)" begin
    c = Counter(3)
    out = collect(c)
    @test (length(out) == 3)
    @test (out[1] == 1001)
    @test (out[2] == 1002)
    @test (out[3] == 1003)
end

true
