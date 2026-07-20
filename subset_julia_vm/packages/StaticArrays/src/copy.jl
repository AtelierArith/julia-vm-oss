function Base.copy(x::SVector{N,T}) where {N,T}
    out = Vector{T}(undef, length(x))
    for i in 1:length(x)
        out[i] = x[i]
    end
    return out
end
