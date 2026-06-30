# Issue #6237: repeated T across Type{T} and Matrix{T} is a diagonal
# specificity relation.

using Test

type_matrix_diagonal_6237(::Type{T}, ::Matrix{T}) where {T<:Real} = :type_mat_same
type_matrix_diagonal_6237(::Type{Integer}, ::Matrix{<:Real}) = :type_integer_mat_real

function type_matrix_diagonal_via_any_6237(t, x)
    tt::Any = t
    xx::Any = x
    type_matrix_diagonal_6237(tt, xx)
end

@testset "Type/matrix diagonal specificity (Issue #6237)" begin
    @test type_matrix_diagonal_6237(Int64, [1 2]) === :type_mat_same
    @test type_matrix_diagonal_6237(Integer, [1 2]) === :type_integer_mat_real
    @test type_matrix_diagonal_6237(Float64, [1.0 2.0]) === :type_mat_same

    @test type_matrix_diagonal_via_any_6237(Int64, [1 2]) === :type_mat_same
    @test type_matrix_diagonal_via_any_6237(Integer, [1 2]) === :type_integer_mat_real
    @test type_matrix_diagonal_via_any_6237(Float64, [1.0 2.0]) === :type_mat_same
end

type_matrix_diagonal_6237(Int64, [1 2]) === :type_mat_same &&
    type_matrix_diagonal_6237(Integer, [1 2]) === :type_integer_mat_real &&
    type_matrix_diagonal_6237(Float64, [1.0 2.0]) === :type_mat_same &&
    type_matrix_diagonal_via_any_6237(Int64, [1 2]) === :type_mat_same &&
    type_matrix_diagonal_via_any_6237(Integer, [1 2]) === :type_integer_mat_real &&
    type_matrix_diagonal_via_any_6237(Float64, [1.0 2.0]) === :type_mat_same
