using LinearAlgebra

function symtridiagonal_matrix_eigvals_contract_8401()
    S = SymTridiagonal([0.0, 0.0, 0.0], [0.5, 0.5])
    M = Matrix(S)
    vals = eigvals(S, 1:2)

    matrix_ok = size(M) == (3, 3) &&
        M[1, 1] == 0.0 &&
        M[1, 2] == 0.5 &&
        M[2, 1] == 0.5 &&
        M[2, 2] == 0.0 &&
        M[2, 3] == 0.5 &&
        M[3, 2] == 0.5

    eigvals_ok = length(vals) == 2 &&
        abs(real(vals[1]) + sqrt(0.5)) < 1e-12 &&
        abs(real(vals[2])) < 1e-12 &&
        abs(imag(vals[1])) < 1e-12 &&
        abs(imag(vals[2])) < 1e-12

    matrix_ok && eigvals_ok
end

symtridiagonal_matrix_eigvals_contract_8401()
