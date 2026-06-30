using Test
using LinearAlgebra

import Base: \

\(A::Matrix{Float64}, b::Vector{Float64}) = :user_ldiv_4020

ldiv_any_4020(A::Any, b::Any) = A \ b

@testset "left division dispatches before VM fallback (Issue #4020, #4353)" begin
    A = [1.0 0.0; 0.0 1.0]
    b = [2.0, 3.0]

    @test A \ b === :user_ldiv_4020
    @test ldiv_any_4020(A, b) === :user_ldiv_4020

    @test 2 \ 4 == 2.0
    @test ldiv_any_4020(2, 4) == 2.0
end

true
