# Issue #6693: Dict keys that are tuples (including tuples containing immutable
# structs such as `OneTo`) must work through the `d[(...)]` bracket-indexing
# syntax and must hash consistently.
#
# Two distinct root causes were fixed:
#   (A) Lowering flattened a parenthesized `d[(1, 2)]` into two indices (as if it
#       were `d[1, 2]` multi-dimensional array indexing), so the tuple key never
#       reached `getindex(::Dict, key)`. A `TupleExpression` inside `[]` is now a
#       single tuple index, matching upstream.
#   (B) `hash`/`_hash` hashed tuple elements and structs via their `Debug` string,
#       which for a heap `StructRef` is the heap index — so two separately
#       constructed equal struct keys got different hashes and lookups missed
#       them. The hash path now resolves struct refs structurally (same class as
#       the #6685 tuple-`==` fix).

checks = Bool[]

# (A) primitive tuple keys via bracket syntax
d = Dict((1, 2) => 10, (3, 4) => 20)
push!(checks, d[(1, 2)] == 10)
push!(checks, d[(3, 4)] == 20)
push!(checks, haskey(d, (1, 2)))
push!(checks, !haskey(d, (9, 9)))
push!(checks, get(d, (1, 2), -1) == 10)
push!(checks, get(d, (9, 9), -1) == -1)

# one-element tuple key (must not be unwrapped to a scalar)
d1 = Dict((5,) => 7)
push!(checks, d1[(5,)] == 7)
push!(checks, haskey(d1, (5,)))

# setindex! with a tuple key
ds = Dict{Tuple{Int,Int},Int}()
ds[(3, 4)] = 88
ds[(3, 4)] = 99
push!(checks, ds[(3, 4)] == 99)
push!(checks, length(ds) == 1)

# (B) struct elements inside a tuple key: separately constructed equal keys
d2 = Dict((Base.OneTo(3),) => 10)
push!(checks, haskey(d2, (Base.OneTo(3),)))
push!(checks, d2[(Base.OneTo(3),)] == 10)
push!(checks, !haskey(d2, (Base.OneTo(4),)))

d3 = Dict((1:3,) => 100, (4:6,) => 200)
push!(checks, d3[(1:3,)] == 100)
push!(checks, d3[(4:6,)] == 200)

# multi-element struct tuple key
d4 = Dict((Base.OneTo(2), Base.OneTo(2)) => 5)
push!(checks, d4[(Base.OneTo(2), Base.OneTo(2))] == 5)

# hash consistency (the root cause of the missed lookups)
push!(checks, hash((1, 2)) == hash((1, 2)))
push!(checks, hash(Base.OneTo(3)) == hash(Base.OneTo(3)))
push!(checks, hash((Base.OneTo(3),)) == hash((Base.OneTo(3),)))
push!(checks, hash((1:3,)) == hash((1:3,)))
push!(checks, hash(Base.OneTo(3)) != hash(Base.OneTo(4)))

all(checks)
