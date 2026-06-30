using Test
using LinearAlgebra

import LinearAlgebra: diag, ldiv!, mul!, opnorm, rdiv!, tr

mul!(C::Matrix{Float64}, A::Matrix{Float64}, B::Matrix{Float64}) = :user_mulbang_4020
ldiv!(A::Matrix{Float64}, b::Vector{Float64}) = :user_ldivbang_4020
rdiv!(A::Matrix{Float64}, B::Matrix{Float64}) = :user_rdivbang_4020
tr(A::Matrix{Float64}) = :user_tr_4020
opnorm(A::Matrix{Float64}) = :user_opnorm_4020
diag(A::Matrix{Float64}) = :user_diag_4020

@testset "matrix literal direct dispatch before LinearAlgebra fallback (Issue #4020)" begin
    A = [1.0 0.0; 0.0 1.0]
    B = [2.0 0.0; 0.0 2.0]
    C = [0.0 0.0; 0.0 0.0]
    b = [1.0, 2.0]

    @test mul!(C, A, B) === :user_mulbang_4020
    @test ldiv!(A, b) === :user_ldivbang_4020
    @test rdiv!(A, B) === :user_rdivbang_4020
    @test tr(A) === :user_tr_4020
    @test opnorm(A) === :user_opnorm_4020
    @test diag(A) === :user_diag_4020
end

true
