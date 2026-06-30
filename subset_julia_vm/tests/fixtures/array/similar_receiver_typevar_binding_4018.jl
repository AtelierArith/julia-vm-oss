using Test

function array_receiver_similar_type_4018(a::Array{T}) where T
    b = similar(a, 2)
    b[1] = zero(T)
    b[2] = one(T)
    return eltype(b) === T && b[1] == zero(T) && b[2] == one(T)
end

function memory_wrapper_receiver_similar_type_4018()
    mem = Memory{Int64}(undef, 3)
    a = Base.wrap(Array, mem, (3,))
    return array_receiver_similar_type_4018(a)
end

@test array_receiver_similar_type_4018([1, 2, 3])
@test memory_wrapper_receiver_similar_type_4018()

true
