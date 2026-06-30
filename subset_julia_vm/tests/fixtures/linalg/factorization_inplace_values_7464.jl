using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

function _matrix_changed(A, B)
    for j in 1:size(A, 2)
        for i in 1:size(A, 1)
            if A[i, j] != B[i, j]
                return true
            end
        end
    end
    return false
end

A = [1.0 2.0; 3.0 4.0]
SPD = [2.0 0.0; 0.0 3.0]

S = svdvals(A)
svdvals_ok = length(S) == 2 &&
    _close(S[1], 5.464985704219043) &&
    _close(S[2], 0.36596619062625746)

A_svdvals = copy(A)
A_svdvals_before = copy(A_svdvals)
S_bang = svdvals!(A_svdvals)
svdvals_bang_ok = length(S_bang) == 2 &&
    _close(S_bang[1], S[1]) &&
    _close(S_bang[2], S[2]) &&
    _matrix_changed(A_svdvals, A_svdvals_before)

A_svd = copy(A)
A_svd_before = copy(A_svd)
F_svd = svd!(A_svd)
svd_bang_ok = F_svd isa Factorization &&
    _close(F_svd.S[1], S[1]) &&
    _matrix_changed(A_svd, A_svd_before)

A_lu = copy(A)
A_lu_before = copy(A_lu)
F_lu = lu!(A_lu)
lu_bang_ok = F_lu isa Factorization &&
    length(F_lu.p) == 2 &&
    _matrix_changed(A_lu, A_lu_before)

A_qr = copy(A)
A_qr_before = copy(A_qr)
F_qr = qr!(A_qr)
qr_bang_ok = F_qr isa Factorization &&
    size(F_qr.R, 1) == 2 &&
    _matrix_changed(A_qr, A_qr_before)

A_eigen = copy(A)
A_eigen_before = copy(A_eigen)
F_eigen = eigen!(A_eigen)
eigen_bang_ok = F_eigen isa Factorization &&
    length(F_eigen.values) == 2 &&
    _close(real(F_eigen.values[1]), -0.3722813232690143) &&
    _close(real(F_eigen.values[2]), 5.372281323269014) &&
    _matrix_changed(A_eigen, A_eigen_before)

A_eigvals = copy(A)
A_eigvals_before = copy(A_eigvals)
values = eigvals!(A_eigvals)
eigvals_bang_ok = length(values) == 2 &&
    _close(real(values[1]), -0.3722813232690143) &&
    _close(real(values[2]), 5.372281323269014) &&
    _matrix_changed(A_eigvals, A_eigvals_before)

A_chol = copy(SPD)
A_chol_before = copy(A_chol)
F_chol = cholesky!(A_chol)
cholesky_bang_ok = F_chol isa Factorization &&
    _close(F_chol.U[1, 1], sqrt(2.0)) &&
    _matrix_changed(A_chol, A_chol_before)

A_posdef = copy(SPD)
A_posdef_before = copy(A_posdef)
posdef_bang_ok = isposdef!(A_posdef) == true &&
    _matrix_changed(A_posdef, A_posdef_before)

not_posdef = [1.0 2.0; 2.0 1.0]
not_posdef_before = copy(not_posdef)
not_posdef_ok = isposdef!(not_posdef) == false &&
    _matrix_changed(not_posdef, not_posdef_before) &&
    _close(not_posdef[2, 2], -3.0)

svdvals_ok &&
    svdvals_bang_ok &&
    svd_bang_ok &&
    lu_bang_ok &&
    qr_bang_ok &&
    eigen_bang_ok &&
    eigvals_bang_ok &&
    cholesky_bang_ok &&
    posdef_bang_ok &&
    not_posdef_ok
