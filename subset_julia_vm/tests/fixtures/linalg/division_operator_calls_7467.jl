using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

A = [1.0 0.0; 0.0 2.0]
b = [1.0, 4.0]
row = [1.0 4.0]

x_call = \(A, b)
x_infix = A \ b
ldiv_ok = length(x_call) == 2 &&
    _close(x_call[1], 1.0) &&
    _close(x_call[2], 2.0) &&
    _close(x_infix[1], x_call[1]) &&
    _close(x_infix[2], x_call[2])

r_call = /(row, A)
r_infix = row / A
rdiv_ok = size(r_call) == (1, 2) &&
    _close(r_call[1, 1], 1.0) &&
    _close(r_call[1, 2], 2.0) &&
    _close(r_infix[1, 1], r_call[1, 1]) &&
    _close(r_infix[1, 2], r_call[1, 2])

SPD = [4.0 0.0; 0.0 9.0]
cf = cholesky(SPD)
cx = cf \ [8.0, 27.0]
factorization_ok = length(cx) == 2 &&
    _close(cx[1], 2.0) &&
    _close(cx[2], 3.0)

ldiv_ok && rdiv_ok && factorization_ok
