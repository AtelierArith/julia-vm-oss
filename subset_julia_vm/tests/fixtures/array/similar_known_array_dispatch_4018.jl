using Test

import Base: similar

function similar(a::Vector{Int64}, n::Int64)
    out = Vector{Int64}(undef, n)
    fill!(out, 4018)
    return out
end

function similar_known_array_dispatch_4018()
    a = [1, 2, 3]
    b = similar(a, 2)
    return typeof(b) === Vector{Int64} && length(b) == 2 && b[1] == 4018 && b[2] == 4018
end

@test similar_known_array_dispatch_4018()

true
