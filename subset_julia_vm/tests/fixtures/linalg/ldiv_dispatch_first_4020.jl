using Test
using LinearAlgebra

import Base: \

\(A::Matrix{Float64}, b::Vector{Float64}) = :user_ldiv_4020

ldiv_any_4020(A::Any, b::Any) = A \ b

# Global helpers keep the operand side-effect log observable after evaluation.
ldiv_events_11240 = Int[]
ldiv_lhs_11240() = (push!(ldiv_events_11240, 1); 2)
ldiv_rhs_11240() = (push!(ldiv_events_11240, 2); 8)
ldiv_nonnumeric_lhs_11240() = (push!(ldiv_events_11240, 1); "a")
ldiv_nonnumeric_rhs_11240() = (push!(ldiv_events_11240, 2); "b")

@testset "left division dispatches before VM fallback (Issue #4020, #4353)" begin
    A = [1.0 0.0; 0.0 1.0]
    b = [2.0, 3.0]

    @test A \ b === :user_ldiv_4020
    @test ldiv_any_4020(A, b) === :user_ldiv_4020

    @test 2 \ 4 == 2.0
    @test ldiv_any_4020(2, 4) == 2.0

    # Scalar `\` is numerically rhs/lhs, but Julia still evaluates operands
    # left-to-right before performing that division (Issue #11240 review
    # regression).
    empty!(ldiv_events_11240)
    @test ldiv_lhs_11240() \ ldiv_rhs_11240() == 4.0
    @test ldiv_events_11240 == [1, 2]

    # Evaluation order is also observable when no applicable method exists and
    # the terminal scalar fallback raises after evaluating both operands.
    empty!(ldiv_events_11240)
    try
        ldiv_nonnumeric_lhs_11240() \ ldiv_nonnumeric_rhs_11240()
    catch
    end
    @test ldiv_events_11240 == [1, 2]
end

true
