using Test
using LinearAlgebra

import LinearAlgebra: inv
import LinearAlgebra: eigen
import LinearAlgebra: svd
import LinearAlgebra: qr
import LinearAlgebra: eigvals
import LinearAlgebra: cholesky
import LinearAlgebra: cond

inv(A::Array) = :user_inv
eigen(A::Array) = :user_eigen
svd(A::Array) = (U=A, S=[-4020.0], V=A, Vt=A)
qr(A::Array) = :user_qr
eigvals(A::Array) = :user_eigvals
cholesky(A::Array) = :user_cholesky
cond(A::Array) = :user_cond

@testset "LinearAlgebra direct builtin dispatch-first routing (Issue #4020)" begin
    A = [1.0 0.0; 0.0 1.0]

    @test inv(A) == :user_inv
    @test LinearAlgebra.inv(A) == :user_inv

    @test eigen(A) == :user_eigen
    @test LinearAlgebra.eigen(A) == :user_eigen

    @test svd(A).S[1] == -4020.0
    @test LinearAlgebra.svd(A).S[1] == -4020.0

    @test qr(A) == :user_qr
    @test LinearAlgebra.qr(A) == :user_qr

    @test eigvals(A) == :user_eigvals
    @test LinearAlgebra.eigvals(A) == :user_eigvals

    @test cholesky(A) == :user_cholesky
    @test LinearAlgebra.cholesky(A) == :user_cholesky

    @test cond(A) == :user_cond
    @test LinearAlgebra.cond(A) == :user_cond
end

true
