# Minimal combinations iterator adapted from upstream Combinatorics.jl.

struct Combinations
    n::Int
    t::Int
end

function _combination_state(t::Int)
    s = Int64[]
    for i in 1:t
        push!(s, min(t - 1, i))
    end
    return s
end

Base.iterate(c::Combinations) = iterate(c, _combination_state(c.t))

function Base.iterate(c::Combinations, s)
    if c.t < 0 || c.t > c.n
        return nothing
    end
    if c.t == 0
        isempty(s) && return (Int[], [1])
        return nothing
    end
    for i in c.t:-1:1
        s[i] += 1
        if s[i] > c.n - (c.t - i)
            continue
        end
        for j in i+1:c.t
            s[j] = s[j - 1] + 1
        end
        break
    end
    s[1] > c.n - c.t + 1 && return nothing
    return (copy(s), s)
end

Base.length(c::Combinations) = c.t < 0 || c.t > c.n ? 0 : binomial(c.n, c.t)

struct IndexedCombinations
    a
    t::Int
end

Base.iterate(c::IndexedCombinations) = iterate(c, _combination_state(c.t))

function Base.iterate(c::IndexedCombinations, state)
    next = iterate(Combinations(length(c.a), c.t), state)
    next === nothing && return nothing
    inds = next[1]
    out = Int[]
    for i in 1:length(inds)
        push!(out, c.a[inds[i]])
    end
    return (out, next[2])
end

Base.length(c::IndexedCombinations) = length(Combinations(length(c.a), c.t))

function Base.collect(c::IndexedCombinations)
    out = []
    state = iterate(c)
    while state !== nothing
        push!(out, state[1])
        state = iterate(c, state[2])
    end
    return out
end

function combinations(a, t::Integer)
    k = t < 0 ? length(a) + 1 : Int(t)
    return IndexedCombinations(a, k)
end
