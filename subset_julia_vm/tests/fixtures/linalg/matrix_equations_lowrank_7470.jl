using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

function _matrix_close(A, B)
    if size(A, 1) != size(B, 1) || size(A, 2) != size(B, 2)
        return false
    end
    for j in 1:size(A, 2)
        for i in 1:size(A, 1)
            if !_close(A[i, j], B[i, j])
                return false
            end
        end
    end
    return true
end

A = [1.0 0.0; 0.0 2.0]
B = [1.0 0.0; 0.0 1.0]
X = lyap(A, B)
lyap_expected = reshape([-0.5, -0.0, -0.0, -0.25], 2, 2)
lyap_ok = _matrix_close(X, lyap_expected)

M = [1.0 2.0; 3.0 4.0]
condskeel_ok = _close(condskeel(M), 12.999999999999996) &&
    _close(condskeel(M, [1.0, 2.0]), 10.499999999999998)

S = [1.0 0.0; 0.0 2.0]
T = [3.0 0.0; 0.0 4.0]
C = [4.0 6.0; 8.0 12.0]
Y = sylvester(S, T, C)
sylvester_expected = reshape([-1.0, -1.6, -1.2, -2.0], 2, 2)
sylvester_ok = _matrix_close(Y, sylvester_expected) &&
    _close(sylvester(1.0, 3.0, 4.0), -1.0)

F = cholesky([4.0 0.0; 0.0 9.0])
U = lowrankupdate(F, [1.0, 2.0])
update_ok = U isa Cholesky &&
    _matrix_close(U.L, [2.23606797749979 0.0; 0.8944271909999159 3.492849839314596]) &&
    _matrix_close(U.U, [2.23606797749979 0.8944271909999159; 0.0 3.492849839314596])

D = lowrankdowndate(U, [1.0, 2.0])
downdate_ok = D isa Cholesky &&
    _matrix_close(D.L, [2.0 0.0; 0.0 2.9999999999999996]) &&
    _matrix_close(D.U, [2.0 0.0; 0.0 2.9999999999999996])

F2 = cholesky([4.0 0.0; 0.0 9.0])
lowrankupdate!(F2, [1.0, 2.0])
mutating_ok = _matrix_close(F2.L, U.L) && _matrix_close(F2.U, U.U)

lyap_ok &&
    condskeel_ok &&
    sylvester_ok &&
    update_ok &&
    downdate_ok &&
    mutating_ok
