# Issue #6251: anonymous bounded tuple slots are independent fallback bounds,
# not a repeated diagonal binding.

using Test

tuple_bounded_fallback_after_diagonal_6251(::Tuple{T,T}) where {T<:Real} = :diag
tuple_bounded_fallback_after_diagonal_6251(::Tuple{<:Real,<:Real}) = :broad

function tuple_bounded_fallback_after_diagonal_via_any_6251(x)
    xx::Any = x
    tuple_bounded_fallback_after_diagonal_6251(xx)
end

@testset "Tuple bounded fallback after diagonal miss (Issue #6251)" begin
    @test tuple_bounded_fallback_after_diagonal_6251((1, 2)) === :diag
    @test tuple_bounded_fallback_after_diagonal_6251((1, 2.0)) === :broad
    @test tuple_bounded_fallback_after_diagonal_6251((1.0, 2.0)) === :diag
    @test_throws MethodError tuple_bounded_fallback_after_diagonal_6251((1, "x"))

    @test tuple_bounded_fallback_after_diagonal_via_any_6251((1, 2)) === :diag
    @test tuple_bounded_fallback_after_diagonal_via_any_6251((1, 2.0)) === :broad
    @test tuple_bounded_fallback_after_diagonal_via_any_6251((1.0, 2.0)) === :diag
    @test_throws MethodError tuple_bounded_fallback_after_diagonal_via_any_6251((1, "x"))
end

tuple_bounded_fallback_after_diagonal_6251((1, 2)) === :diag &&
    tuple_bounded_fallback_after_diagonal_6251((1, 2.0)) === :broad &&
    tuple_bounded_fallback_after_diagonal_6251((1.0, 2.0)) === :diag &&
    tuple_bounded_fallback_after_diagonal_via_any_6251((1, 2)) === :diag &&
    tuple_bounded_fallback_after_diagonal_via_any_6251((1, 2.0)) === :broad &&
    tuple_bounded_fallback_after_diagonal_via_any_6251((1.0, 2.0)) === :diag
