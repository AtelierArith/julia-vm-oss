using LinearAlgebra

function _mat2_eq(A, a11, a12, a21, a22)
    return A[1, 1] == a11 &&
        A[1, 2] == a12 &&
        A[2, 1] == a21 &&
        A[2, 2] == a22
end

A = [1.0 2.0; 3.0 4.0]
R = [1.0 2.0 3.0; 4.0 5.0 6.0]

d0 = diagind(A)
d1 = diagind(R, 1)
dm1 = diagind(R, -1)
diagind_ok = length(d0) == 2 &&
    d0[1] == 1 &&
    d0[2] == 4 &&
    length(d1) == 2 &&
    d1[1] == 3 &&
    d1[2] == 6 &&
    length(dm1) == 1 &&
    dm1[1] == 2

DV_PARENT = copy(A)
dv = diagview(DV_PARENT)
dv[2] = 9.0
diagview_ok = length(dv) == 2 &&
    dv[1] == 1.0 &&
    dv[2] == 9.0 &&
    DV_PARENT[2, 2] == 9.0

B = similar(A)
transpose!(B, A)
transpose_ok = _mat2_eq(B, 1.0, 3.0, 2.0, 4.0)

C = similar(A)
adjoint!(C, A)
adjoint_ok = _mat2_eq(C, 1.0, 3.0, 2.0, 4.0)

T = copy(A)
triu!(T)
triu_ok = _mat2_eq(T, 1.0, 2.0, 0.0, 4.0)

L = copy(A)
tril!(L)
tril_ok = _mat2_eq(L, 1.0, 0.0, 3.0, 4.0)

CT = similar(A)
copy_transpose!(CT, 1:2, 1:2, A, 1:2, 1:2)
copy_transpose_ok = _mat2_eq(CT, 1.0, 3.0, 2.0, 4.0)

CA = similar(A)
copy_adjoint!(CA, 1:2, 1:2, A, 1:2, 1:2)
copy_adjoint_ok = _mat2_eq(CA, 1.0, 3.0, 2.0, 4.0)

CU = zeros(2, 2)
copytrito!(CU, A, 'U')
copytrito_ok = _mat2_eq(CU, 1.0, 2.0, 0.0, 4.0)

CD = zeros(2, 2)
copyto!(CD, Diagonal([5.0, 6.0]))
copyto_diag_ok = _mat2_eq(CD, 5.0, 0.0, 0.0, 6.0)

diagind_ok &&
    diagview_ok &&
    transpose_ok &&
    adjoint_ok &&
    triu_ok &&
    tril_ok &&
    copy_transpose_ok &&
    copy_adjoint_ok &&
    copytrito_ok &&
    copyto_diag_ok
