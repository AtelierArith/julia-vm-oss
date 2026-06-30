using Test

import Base: similar

function similar(a::Vector{Int64}, n::Int64)
    out = Vector{Int64}(undef, n)
    fill!(out, 4018)
    return out
end

function similar_any_receiver_dispatch_4018(a, n)
    b = similar(a, n)
    return typeof(b) === Vector{Int64} && length(b) == n && b[1] == 4018 && b[n] == 4018
end

@test similar_any_receiver_dispatch_4018([1, 2, 3], 2)

true
