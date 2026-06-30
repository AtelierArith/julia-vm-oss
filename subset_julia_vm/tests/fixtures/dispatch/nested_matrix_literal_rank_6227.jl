# Issue #6227: nested matrix literals must keep Matrix{T}, not collapse to the
# Vector{T} projection used for nested vector literals.

using Test

nested_matrix_literal_rank_6227(::Vector{T}) where {T} = :outer
nested_matrix_literal_rank_6227(::Vector{Matrix{T}}) where {T} = :matrix

function nested_matrix_literal_rank_via_any_6227(x)
    y::Any = x
    nested_matrix_literal_rank_6227(y)
end

@testset "nested Matrix literal rank projection (Issue #6227)" begin
    xs = [[1 2], [3 4]]

    @test typeof(xs) === Vector{Matrix{Int64}}
    @test nested_matrix_literal_rank_6227(xs) === :matrix
    @test nested_matrix_literal_rank_via_any_6227(xs) === :matrix
end

xs_6227 = [[1 2], [3 4]]
typeof(xs_6227) === Vector{Matrix{Int64}} &&
    nested_matrix_literal_rank_6227(xs_6227) === :matrix &&
    nested_matrix_literal_rank_via_any_6227(xs_6227) === :matrix
