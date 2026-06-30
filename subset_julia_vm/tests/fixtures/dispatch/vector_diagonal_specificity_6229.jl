# Issue #6229: a repeated Vector{T} type parameter is more specific than
# independent Vector{<:B} bounds when both runtime vector element types agree.

using Test

vector_diagonal_specificity_6229(::Vector{T}, ::Vector{T}) where {T<:Real} = :same_vec
vector_diagonal_specificity_6229(::Vector{<:Real}, ::Vector{<:Real}) = :real_vecs

function vector_diagonal_specificity_via_any_6229(x, y)
    a::Any = x
    b::Any = y
    vector_diagonal_specificity_6229(a, b)
end

@testset "Vector diagonal specificity (Issue #6229)" begin
    @test vector_diagonal_specificity_6229([1], [2]) === :same_vec
    @test vector_diagonal_specificity_6229([1], [2.0]) === :real_vecs

    @test vector_diagonal_specificity_via_any_6229([1], [2]) === :same_vec
    @test vector_diagonal_specificity_via_any_6229([1], [2.0]) === :real_vecs
end

vector_diagonal_specificity_6229([1], [2]) === :same_vec &&
    vector_diagonal_specificity_6229([1], [2.0]) === :real_vecs &&
    vector_diagonal_specificity_via_any_6229([1], [2]) === :same_vec &&
    vector_diagonal_specificity_via_any_6229([1], [2.0]) === :real_vecs
