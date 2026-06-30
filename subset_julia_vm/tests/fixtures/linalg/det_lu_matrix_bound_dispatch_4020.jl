using LinearAlgebra

import LinearAlgebra: det, lu

det(A::Matrix{<:Integer}) = 40201
det(A::Matrix{<:AbstractFloat}) = 40202
lu(A::Matrix{<:Integer}) = :matrix_integer_lu_4020
lu(A::Matrix{<:AbstractFloat}) = :matrix_float_lu_4020

Ai = [1 2; 3 4]
Af = [1.0 2.0; 3.0 4.0]

if det(Ai) != 40201
    error("det integer dispatch failed")
end
if lu(Ai) !== :matrix_integer_lu_4020
    error("lu integer dispatch failed")
end
if LinearAlgebra.det(Ai) != 40201
    error("qualified det integer dispatch failed")
end
if LinearAlgebra.lu(Ai) !== :matrix_integer_lu_4020
    error("qualified lu integer dispatch failed")
end
if det(Af) != 40202
    error("Matrix{Float64} incorrectly matched Matrix{<:Integer}")
end
if lu(Af) !== :matrix_float_lu_4020
    error("Matrix{Float64} incorrectly matched Matrix{<:Integer} lu")
end

true
