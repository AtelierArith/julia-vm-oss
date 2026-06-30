# Issue #6240: repeated T across Type{T} and AbstractMatrix{T} is a
# diagonal specificity relation for concrete Matrix actuals.

using Test

type_abstract_matrix_diagonal_6240(::Type{T}, ::AbstractMatrix{T}) where {T<:Real} = :type_absmat_same
type_abstract_matrix_diagonal_6240(::Type{Integer}, ::AbstractMatrix{<:Real}) = :type_integer_absmat_real

function type_abstract_matrix_diagonal_via_any_6240(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_matrix_diagonal_6240(tt, xx)
end

type_abstract_matrix_diagonal_exact_6240(::Type{T}, ::AbstractMatrix{T}) where {T<:Real} = :type_absmat_same
type_abstract_matrix_diagonal_exact_6240(::Type{Int64}, ::AbstractMatrix{Int64}) = :type_int_absmat_int

@testset "Type/AbstractMatrix diagonal specificity (Issue #6240)" begin
    @test type_abstract_matrix_diagonal_6240(Int64, [1 2]) === :type_absmat_same
    @test type_abstract_matrix_diagonal_6240(Integer, [1 2]) === :type_integer_absmat_real
    @test type_abstract_matrix_diagonal_6240(Float64, [1.0 2.0]) === :type_absmat_same

    @test type_abstract_matrix_diagonal_via_any_6240(Int64, [1 2]) === :type_absmat_same
    @test type_abstract_matrix_diagonal_via_any_6240(Integer, [1 2]) === :type_integer_absmat_real
    @test type_abstract_matrix_diagonal_via_any_6240(Float64, [1.0 2.0]) === :type_absmat_same

    @test type_abstract_matrix_diagonal_exact_6240(Int64, [1 2]) === :type_int_absmat_int
end

type_abstract_matrix_diagonal_6240(Int64, [1 2]) === :type_absmat_same &&
    type_abstract_matrix_diagonal_6240(Integer, [1 2]) === :type_integer_absmat_real &&
    type_abstract_matrix_diagonal_6240(Float64, [1.0 2.0]) === :type_absmat_same &&
    type_abstract_matrix_diagonal_via_any_6240(Int64, [1 2]) === :type_absmat_same &&
    type_abstract_matrix_diagonal_via_any_6240(Integer, [1 2]) === :type_integer_absmat_real &&
    type_abstract_matrix_diagonal_via_any_6240(Float64, [1.0 2.0]) === :type_absmat_same &&
    type_abstract_matrix_diagonal_exact_6240(Int64, [1 2]) === :type_int_absmat_int
