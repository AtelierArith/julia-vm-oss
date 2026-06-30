using Test

function wrapper_push_dispatch_4018(a)
    b = similar(a, 0)
    push!(b, one(eltype(a)))
    push!(b, one(eltype(a)) + one(eltype(a)))
    return typeof(b) === Vector{eltype(a)} && length(b) == 2 && b[1] == 1 && b[2] == 2
end

@test wrapper_push_dispatch_4018([1, 2, 3])

true
