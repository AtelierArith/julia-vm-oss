function Base.copy(x::SVector)
    out = Float64[]
    for i in 1:length(x)
        push!(out, x[i])
    end
    return out
end
