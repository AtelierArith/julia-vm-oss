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

A = [1.0 2.0; 2.0 3.0]

S = schur(A)
schur_ok = S isa Schur &&
    length(S.values) == 2 &&
    _matrix_close(S.Z * S.T * transpose(S.Z), A) &&
    ordschur(S, [true, false]) isa Schur

S2 = schur!(copy(A))
schur_mutating_ok = S2 isa Schur &&
    length(S2.values) == 2

H = hessenberg(A)
hessenberg_ok = H isa Hessenberg &&
    H.H isa UpperHessenberg &&
    H.H[2, 1] == 2.0 &&
    H.factors[1, 2] == 2.0 &&
    length(H.τ) == 1 &&
    hessenberg!(copy(A)) isa Hessenberg

L = lq(A)
lq_ok = L isa LQ &&
    size(L.factors) == (2, 2) &&
    length(L.τ) == 2 &&
    lq!(copy(A)) isa LQ

ST = SymTridiagonal([2.0, 3.0], [0.5])
D = ldlt(ST)
ldlt_ok = D isa LDLt &&
    D.data isa SymTridiagonal &&
    length(D.data.dv) == 2 &&
    ldlt!(ST) isa LDLt

B = bunchkaufman(A)
bunch_ok = B isa BunchKaufman &&
    size(B.LD) == (2, 2) &&
    length(B.ipiv) == 2 &&
    B.info == 0 &&
    bunchkaufman!(copy(A)) isa BunchKaufman

gen_ok = GeneralizedEigen([1.0, 2.0], A) isa GeneralizedEigen &&
    GeneralizedSchur(A, A, [1.0, 2.0], [1.0, 1.0], A, A) isa GeneralizedSchur &&
    GeneralizedSVD(A, A, A, [1.0], [1.0], 1, 0, A) isa GeneralizedSVD

schur_ok &&
    schur_mutating_ok &&
    hessenberg_ok &&
    lq_ok &&
    ldlt_ok &&
    bunch_ok &&
    gen_ok
