# Issue #6707: `d[k1, k2, ...]` (comma form) on a tuple-keyed Dict is sugar for
# `d[(k1, k2, ...)]`. Upstream defines
#   getindex(t::AbstractDict, k1, k2, ks...) = getindex(t, tuple(k1, k2, ks...))
#   setindex!(t::AbstractDict, v, k1, k2, ks...) = setindex!(t, v, tuple(k1, k2, ks...))
# Previously the comma form fell through to native multi-dim array indexing
# (IndexLoad/IndexStore(N)) and raised a MethodError on a Dict.

checks = Bool[]

# getindex comma form
d = Dict((1, 2) => 10, (3, 4) => 20)
push!(checks, d[1, 2] == 10)
push!(checks, d[3, 4] == 20)
push!(checks, d[(1, 2)] == 10)          # single tuple key still works

# 3-element tuple key
d3 = Dict((1, 2, 3) => 99)
push!(checks, d3[1, 2, 3] == 99)
push!(checks, d3[(1, 2, 3)] == 99)

# typed Dict, comma getindex + setindex!
td = Dict{Tuple{Int,Int},Int}()
td[1, 2] = 99
td[3, 4] = 88
push!(checks, td[1, 2] == 99)
push!(checks, td[3, 4] == 88)
push!(checks, length(td) == 2)
td[1, 2] = 100                          # overwrite via comma form
push!(checks, td[(1, 2)] == 100)
push!(checks, length(td) == 2)

# struct-element tuple keys via comma form (ties into the #6685/#6693 family)
ds = Dict((Base.OneTo(2), Base.OneTo(2)) => 5)
push!(checks, ds[Base.OneTo(2), Base.OneTo(2)] == 5)

# get with a comma key is NOT a thing (get takes a single key); but haskey on a
# tuple key built explicitly still works
push!(checks, haskey(d, (1, 2)))
push!(checks, !haskey(d, (9, 9)))

# Regression: multi-dim ARRAY indexing must be unaffected by the Dict rewrite.
A = [1 2; 3 4]
push!(checks, A[2, 1] == 3)
A[2, 1] = 99
push!(checks, A[2, 1] == 99)
push!(checks, A[1, 2] == 2)

all(checks)
