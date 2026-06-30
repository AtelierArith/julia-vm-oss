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

A = [1.0 2.0; 3.0 4.0]
Asym = [1.0 2.0; 2.0 3.0]

uniform_ok = I isa UniformScaling &&
    I[1, 1] == true &&
    I[1, 2] == false &&
    _matrix_close(I * A, A) &&
    _matrix_close(A * I, A) &&
    (3 * I) isa UniformScaling

S = Symmetric(Asym)
Id2 = [1.0 0.0; 0.0 1.0]
symmetric_ok = S isa Symmetric &&
    size(S) == (2, 2) &&
    S.data[1, 2] == 2.0 &&
    S[2, 1] == 2.0 &&
    _matrix_close(S * Id2, Asym)

Hdata = [1.0 + 0.0im 2.0 + 3.0im; 0.0 + 0.0im 4.0 + 0.0im]
H = Hermitian(Hdata)
hermitian_ok = H isa Hermitian &&
    H[2, 1] == 2.0 - 3.0im &&
    H[1, 2] == 2.0 + 3.0im

U = UpperTriangular(A)
L = LowerTriangular(A)
UU = UnitUpperTriangular(A)
UL = UnitLowerTriangular(A)
UH = UpperHessenberg([1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0])
triangular_ok = U[2, 1] == 0.0 &&
    U[1, 2] == 2.0 &&
    L[1, 2] == 0.0 &&
    L[2, 1] == 3.0 &&
    UU[1, 1] == 1.0 &&
    UU[2, 1] == 0.0 &&
    UL[2, 2] == 1.0 &&
    UL[1, 2] == 0.0 &&
    UH[3, 1] == 0.0 &&
    UH[2, 1] == 4.0

B = Bidiagonal([1.0, 2.0, 3.0], [4.0, 5.0], :U)
T = Tridiagonal([1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0])
ST = SymTridiagonal([1.0, 2.0, 3.0], [4.0, 5.0])
banded_ok = B isa Bidiagonal &&
    size(B) == (3, 3) &&
    B[1, 2] == 4.0 &&
    B[2, 1] == 0.0 &&
    T[2, 1] == 1.0 &&
    T[2, 3] == 7.0 &&
    ST[2, 1] == 4.0 &&
    ST[2, 3] == 5.0

Tr = Transpose(A)
Ad = Adjoint(Hdata)
transpose_ok = Tr isa Transpose &&
    size(Tr) == (2, 2) &&
    Tr[1, 2] == 3.0 &&
    Ad isa Adjoint &&
    Ad[1, 2] == 0.0 - 0.0im &&
    Ad[2, 1] == 2.0 - 3.0im

Id3 = [1.0 0.0 0.0; 0.0 1.0 0.0; 0.0 0.0 1.0]
mul_ok = _matrix_close(U * Id2, [1.0 2.0; 0.0 4.0]) &&
    _matrix_close(Id2 * L, [1.0 0.0; 3.0 4.0]) &&
    _matrix_close(B * Id3, [1.0 4.0 0.0; 0.0 2.0 5.0; 0.0 0.0 3.0])

uniform_ok &&
    symmetric_ok &&
    hermitian_ok &&
    triangular_ok &&
    banded_ok &&
    transpose_ok &&
    mul_ok
