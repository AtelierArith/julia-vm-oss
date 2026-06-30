module StatsBase

using Statistics
using Random

export Weights, FrequencyWeights, ProbabilityWeights,
       weights, Histogram, fit, sample, sample!, countmap,
       mode, modes, skewness, kurtosis, entropy

abstract type AbstractWeights{T<:Real, V<:AbstractVector{T}} end

struct Weights{T<:Real, V<:AbstractVector{T}} <: AbstractWeights{T, V}
    values::V
    sum::T
end
Weights(v::AbstractVector) = Weights{eltype(v), typeof(v)}(v, sum(v))

struct FrequencyWeights{T<:Real, V<:AbstractVector{T}} <: AbstractWeights{T, V}
    values::V
    sum::T
end
FrequencyWeights(v::AbstractVector) = FrequencyWeights{eltype(v), typeof(v)}(v, sum(v))

struct ProbabilityWeights{T<:Real, V<:AbstractVector{T}} <: AbstractWeights{T, V}
    values::V
    sum::T
end
ProbabilityWeights(v::AbstractVector) = ProbabilityWeights{eltype(v), typeof(v)}(v, sum(v))

weights(v::AbstractVector) = Weights(v)
Base.getindex(w::AbstractWeights, i::Int) = w.values[i]
Base.length(w::AbstractWeights) = length(w.values)
Base.sum(w::AbstractWeights) = w.sum

struct Histogram{T<:Real, E}
    edges::E
    weights::Vector{T}
    closed::Symbol
end

function fit(::Type{Histogram}, x::AbstractVector; nbins::Int=10)
    T = eltype(x)
    lo, hi = minimum(x), maximum(x)
    if lo == hi
        edges = (lo, hi)
        return Histogram{T, typeof(edges)}(edges, T[T(length(x))], :left)
    end
    edges = range(lo, hi; length=nbins + 1)
    w = zeros(T, nbins)
    for v in x
        bin = searchsortedlast(edges, v)
        if bin < 1
            bin = 1
        elseif bin > nbins
            bin = nbins
        end
        w[bin] += 1
    end
    return Histogram{T, typeof(edges)}(edges, w, :left)
end

function sample(a::AbstractVector, n::Int; replace::Bool=true)
    if replace
        return [a[Int64(floor(rand() * length(a))) + 1] for _ in 1:n]
    else
        n <= length(a) || error("n must not exceed length(a) when replace=false")
        idx = randperm(length(a))
        return a[idx[1:n]]
    end
end

sample!(a::AbstractVector, x::AbstractVector) = copyto!(x, sample(a, length(x); replace=false))

function countmap(x::AbstractVector)
    d = Dict()
    for v in x
        d[v] = get(d, v, 0) + 1
    end
    return d
end

function mode(x::AbstractVector)
    cm = countmap(x)
    best = collect(keys(cm))[1]
    best_count = cm[best]
    for (k, v) in cm
        if v > best_count
            best = k
            best_count = v
        end
    end
    return best
end

function modes(x::AbstractVector)
    cm = countmap(x)
    maxc = maximum(collect(values(cm)))
    return [k for (k, v) in cm if v == maxc]
end

function entropy(p::AbstractVector{T}) where {T<:Real}
    s = zero(eltype(p))
    z = sum(p)
    for v in p
        if v > 0
            v /= z
            s -= v * log(v)
        end
    end
    return s
end

# Placeholders for moment-based functions (Task 6)
skewness(x) = error("skewness not yet implemented")
kurtosis(x) = error("kurtosis not yet implemented")

end # module StatsBase
