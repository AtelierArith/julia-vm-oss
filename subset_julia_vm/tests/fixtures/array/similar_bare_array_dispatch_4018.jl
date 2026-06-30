using Test

function bare_array_similar_shape_4018(a)
    b = similar(a, 0)
    push!(b, one(eltype(a)))
    push!(b, one(eltype(a)) + one(eltype(a)))
    return typeof(b) === Vector{eltype(a)} && b[1] == one(eltype(a)) && b[2] == one(eltype(a)) + one(eltype(a))
end

function bare_array_similar_tuple_shape_4018(a)
    b = similar(a, (2,))
    b[1] = zero(eltype(a))
    b[2] = one(eltype(a))
    return typeof(b) === Vector{eltype(a)} && b[1] == zero(eltype(a)) && b[2] == one(eltype(a))
end

@test bare_array_similar_shape_4018([1, 2, 3])
@test bare_array_similar_tuple_shape_4018([1, 2, 3])
@test vcat([1, 2], [3, 4]) == [1, 2, 3, 4]

true
