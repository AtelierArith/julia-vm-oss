using LinearAlgebra

A = [1.0 2.0; 3.0 4.0]
SPD = [2.0 0.0; 0.0 3.0]

F_lu = lu(A)
L, U, p = F_lu
lu_ok = F_lu isa Factorization &&
    issuccess(F_lu) &&
    size(F_lu.L, 1) == 2 &&
    size(U, 2) == 2 &&
    length(p) == 2

F_qr = qr(A)
qr_ok = F_qr isa Factorization &&
    size(F_qr.Q, 1) == 2 &&
    size(F_qr.R, 2) == 2

F_chol = cholesky(SPD)
chol_ok = F_chol isa Factorization &&
    issuccess(F_chol) &&
    size(F_chol.L, 1) == 2 &&
    size(F_chol.U, 2) == 2

F_eigen = eigen(SPD)
eigen_ok = F_eigen isa Factorization &&
    length(F_eigen.values) == 2 &&
    size(F_eigen.vectors, 2) == 2

F_svd = svd(A)
svd_ok = F_svd isa Factorization &&
    size(F_svd.U, 1) == 2 &&
    length(F_svd.S) == 2 &&
    size(F_svd.Vt, 2) == 2

lu_ok && qr_ok && chol_ok && eigen_ok && svd_ok
