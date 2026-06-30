using Test

struct MutationDispatchBox3911
    n::Int64
end

function Base.pushfirst!(b::MutationDispatchBox3911, v::Int64)
    return b.n + v + 1000
end

function Base.popfirst!(b::MutationDispatchBox3911)
    return b.n + 2000
end

function Base.insert!(b::MutationDispatchBox3911, i::Int64, v::Int64)
    return b.n + i + v + 3000
end

function Base.deleteat!(b::MutationDispatchBox3911, i::Int64)
    return b.n + i + 4000
end

box = MutationDispatchBox3911(10)

@test Base.pushfirst!(box, 2) == 1012
@test Base.popfirst!(box) == 2010
@test Base.insert!(box, 3, 4) == 3017
@test Base.deleteat!(box, 5) == 4015

true
