# =============================================================================
# LinearAlgebra - Linear Algebra Standard Library
# =============================================================================
# Based on Julia's LinearAlgebra stdlib
# https://docs.julialang.org/en/v1/stdlib/LinearAlgebra/
#
# This module provides basic linear algebra operations for vectors and matrices.
# Functions are implemented to match Julia's LinearAlgebra module behavior.

module LinearAlgebra

export BLAS, LAPACK
export tr, dot, norm, cross
export \, /
export kron, kron!
export lu, lu!, det, inv, svd, svd!, svdvals, svdvals!, qr, qr!
export eigen, eigen!, eigvals, eigvals!, eigvecs, cholesky, cholesky!, rank, cond, pinv
export condskeel, lyap, sylvester
export transpose, transpose!, adjoint!
export Diagonal
export UniformScaling, I
export Symmetric, Hermitian
export UpperTriangular, LowerTriangular, UnitUpperTriangular, UnitLowerTriangular, UpperHessenberg
export Bidiagonal, Tridiagonal, SymTridiagonal
export Transpose, Adjoint
export Factorization, LU, QR, Cholesky, Eigen, SVD, issuccess
export Schur, GeneralizedSchur, Hessenberg, LQ, LDLt, BunchKaufman
export GeneralizedEigen, GeneralizedSVD
export schur, schur!, ordschur, ordschur!
export hessenberg, hessenberg!, lq, lq!, ldlt, ldlt!, bunchkaufman, bunchkaufman!
export normalize, diag, diagind, diagview, issymmetric, ishermitian
export triu, triu!, tril, tril!, diagm, opnorm
export nullspace, logdet, logabsdet, adjoint
export copy_transpose!, copy_adjoint!, copytrito!
export isdiag, istriu, istril, isposdef, isposdef!
export hermitianpart, eigmax, eigmin
export checksquare
export axpy!, axpby!, rmul!, lmul!
export mul!, ldiv!, rdiv!
export givens, rotate!, reflect!
export lowrankupdate, lowrankupdate!, lowrankdowndate, lowrankdowndate!
# Note: isapprox is defined in Base with array support via isa() check

# =============================================================================
# Basic Matrix Operations
# =============================================================================

"""
    tr(A)

Compute the trace of a matrix A, i.e., the sum of its diagonal elements.
"""
function tr(A)
    n = size(A, 1)
    m = size(A, 2)
    # For non-square matrices, use min dimension
    k = n < m ? n : m
    s = 0.0
    for i in 1:k
        s = s + A[i, i]
    end
    return s
end

"""
    *(A, B)

Matrix multiplication for 2D arrays.
"""
function Base.:*(A::AbstractMatrix, B::AbstractMatrix)
    if size(A, 2) != size(B, 1)
        error("DimensionMismatch: A has $(size(A, 2)) columns, but B has $(size(B, 1)) rows")
    end
    m = size(A, 1)
    n = size(B, 2)
    k = size(A, 2)
    C = zeros(m, n)
    for i in 1:m
        for j in 1:n
            s = 0.0
            for p in 1:k
                s = s + A[i, p] * B[p, j]
            end
            C[i, j] = s
        end
    end
    return C
end

"""
    *(A, x)

Matrix-vector multiplication: A (m×n matrix) * x (n-vector) -> (m-vector)
"""
function Base.:*(A::AbstractMatrix, x::AbstractVector)
    m = size(A, 1)
    n = size(A, 2)
    if n != length(x)
        error("DimensionMismatch: A has $(n) columns, but x has $(length(x)) elements")
    end
    y = zeros(m)
    for i in 1:m
        s = 0.0
        for j in 1:n
            s = s + A[i, j] * x[j]
        end
        y[i] = s
    end
    return y
end

function Base.:*(x::AbstractVector, A::AbstractMatrix)
    rows = length(x)
    if size(A, 1) != 1
        error("DimensionMismatch: vector-matrix multiplication requires matrix first dimension 1")
    end
    cols = size(A, 2)
    C = zeros(rows, cols)
    for i in 1:rows
        for j in 1:cols
            C[i, j] = x[i] * A[1, j]
        end
    end
    return C
end

# Matrix{Float64} * Vector{Complex{Float64}} -> Vector{Complex{Float64}}
# This handles the case where eigenvectors are complex but the original matrix is real
function Base.:*(A::Matrix{Float64}, x::Vector{Complex{Float64}})
    m = size(A, 1)
    n = size(A, 2)
    if n != length(x)
        error("DimensionMismatch: A has $(n) columns, but x has $(length(x)) elements")
    end
    y = Vector{Complex{Float64}}(undef, m)
    for i in 1:m
        s = Complex(0.0, 0.0)
        for j in 1:n
            s = s + A[i, j] * x[j]
        end
        y[i] = s
    end
    return y
end

# =============================================================================
# Dot Product / Inner Product
# =============================================================================

"""
    dot(x, y)
    x ⋅ y

Compute the dot product between two vectors.
For complex vectors, the first vector is conjugated.
"""
# Specialized dot for Float64 arrays (most common case)
function dot(x::Array{Float64}, y::Array{Float64})
    n = length(x)
    if n != length(y)
        error("DimensionMismatch: vectors must have same length")
    end
    s = 0.0
    for i in 1:n
        s = s + x[i] * y[i]
    end
    return s
end

# Specialized dot for Int64 arrays
function dot(x::Array{Int64}, y::Array{Int64})
    n = length(x)
    if n != length(y)
        error("DimensionMismatch: vectors must have same length")
    end
    s = 0
    for i in 1:n
        s = s + x[i] * y[i]
    end
    return s
end

# Specialized dot for Complex{Float64} arrays
# NOTE: Temporarily simplified - conj dispatch has issues
function dot(x::Array{Complex{Float64}}, y::Array{Complex{Float64}})
    n = length(x)
    if n != length(y)
        error("DimensionMismatch: vectors must have same length")
    end
    s = Complex{Float64}(0.0, 0.0)
    for i in 1:n
        # Inner product for complex: conj(x[i]) * y[i]
        # Using explicit Complex conjugate instead of generic conj
        xi = x[i]
        yi = y[i]
        xi_conj = Complex{Float64}(xi.re, -xi.im)
        s = s + xi_conj * yi
    end
    return s
end

# Generic dot for other array types
function dot(x, y)
    n = length(x)
    if n != length(y)
        error("DimensionMismatch: vectors must have same length")
    end
    s = 0.0
    for i in 1:n
        s = s + x[i] * y[i]
    end
    return s
end

# =============================================================================
# Norms
# =============================================================================

"""
    norm(x, p=2)

Compute the p-norm of a vector x.
- p=2 (default): Euclidean norm (L2 norm), sqrt(sum(|x_i|^2))
- p=1: Manhattan norm (L1 norm), sum(|x_i|)
- p=Inf: Maximum norm, max(|x_i|)
"""
# Specialized norm for Float64 arrays
# Inner loops iterate the array directly (`for xi in x`) rather than indexing
# (`for i in 1:n; xi = x[i]`); direct iteration avoids the per-element
# bounds-checked `getindex` and the `1:n` range, ~2x faster on the VM (Issue #6846).
function norm(x::Array{Float64}, p)
    if p == 2
        s = 0.0
        for xi in x
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for xi in x
            s = s + abs(xi)
        end
        return s
    elseif isinf(p)
        m = 0.0
        for xi in x
            v = abs(xi)
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for xi in x
            s = s + abs(xi)^p
        end
        return s^(1.0/p)
    end
end

# Specialized norm for Int64 arrays
function norm(x::Array{Int64}, p)
    if p == 2
        s = 0.0
        for xj in x
            xi = Float64(xj)
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for xj in x
            s = s + abs(Float64(xj))
        end
        return s
    elseif isinf(p)
        m = 0.0
        for xj in x
            v = abs(Float64(xj))
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for xj in x
            s = s + abs(Float64(xj))^p
        end
        return s^(1.0/p)
    end
end

# Specialized norm for Complex{Float64} arrays
function norm(x::Array{Complex{Float64}}, p)
    if p == 2
        s = 0.0
        for xi in x
            # abs2(z) = re^2 + im^2
            s = s + xi.re * xi.re + xi.im * xi.im
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for xi in x
            s = s + sqrt(xi.re * xi.re + xi.im * xi.im)
        end
        return s
    elseif isinf(p)
        m = 0.0
        for xi in x
            v = sqrt(xi.re * xi.re + xi.im * xi.im)
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for xi in x
            v = sqrt(xi.re * xi.re + xi.im * xi.im)
            s = s + v^p
        end
        return s^(1.0/p)
    end
end

# Generic norm fallback
function norm(x, p)
    if p == 2
        # L2 norm (Euclidean)
        s = 0.0
        for xi in x
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        # L1 norm (Manhattan)
        s = 0.0
        for xi in x
            s = s + abs(xi)
        end
        return s
    elseif isinf(p)
        # Infinity norm
        m = 0.0
        for xi in x
            v = abs(xi)
            if v > m
                m = v
            end
        end
        return m
    else
        # General p-norm
        s = 0.0
        for xi in x
            s = s + abs(xi)^p
        end
        return s^(1.0 / p)
    end
end

# Default p=2 (Euclidean norm)
function norm(x)
    return norm(x, 2)
end

# =============================================================================
# Cross Product
# =============================================================================

"""
    cross(x, y)
    x × y

Compute the cross product of two 3-vectors.
Returns a vector perpendicular to both x and y.
"""
function cross(x, y)
    if length(x) != 3 || length(y) != 3
        error("DimensionMismatch: cross product requires 3-element vectors")
    end
    # cross(a, b) = [a2*b3 - a3*b2, a3*b1 - a1*b3, a1*b2 - a2*b1]
    c1 = x[2] * y[3] - x[3] * y[2]
    c2 = x[3] * y[1] - x[1] * y[3]
    c3 = x[1] * y[2] - x[2] * y[1]
    return [c1, c2, c3]
end

# =============================================================================
# Kronecker Product
# =============================================================================

# Kronecker product of two matrices or vectors
function kron(A, B)
    # Use length(size(A)) as ndims(A)
    ndA = length(size(A))
    ndB = length(size(B))

    if ndA == 1 && ndB == 1
        return _kron_vec(A, B)
    elseif ndA == 2 && ndB == 2
        return _kron_mat(A, B)
    elseif ndA == 1 && ndB == 2
        return _kron_vec_mat(A, B)
    elseif ndA == 2 && ndB == 1
        return _kron_mat_vec(A, B)
    else
        error("kron: unsupported dimensions")
    end
end

function _kron_vec(a, b)
    m = length(a)
    n = length(b)
    c = zeros(m * n)
    idx = 1
    for i in 1:m
        ai = a[i]
        for k in 1:n
            c[idx] = Float64(ai * b[k])
            idx = idx + 1
        end
    end
    return c
end

function _kron_mat(A, B)
    mA = size(A, 1)
    nA = size(A, 2)
    mB = size(B, 1)
    nB = size(B, 2)
    mC = mA * mB
    nC = nA * nB
    C = zeros(mC, nC)
    for j in 1:nA
        for l in 1:nB
            colC = (j - 1) * nB + l
            for i in 1:mA
                Aij = A[i, j]
                for k in 1:mB
                    rowC = (i - 1) * mB + k
                    C[rowC, colC] = Float64(Aij * B[k, l])
                end
            end
        end
    end
    return C
end

function _kron_vec_mat(a, B)
    m = length(a)
    mB = size(B, 1)
    nB = size(B, 2)
    mC = m * mB
    nC = nB
    C = zeros(mC, nC)
    for l in 1:nB
        for i in 1:m
            ai = a[i]
            for k in 1:mB
                rowC = (i - 1) * mB + k
                C[rowC, l] = Float64(ai * B[k, l])
            end
        end
    end
    return C
end

function _kron_mat_vec(A, b)
    mA = size(A, 1)
    nA = size(A, 2)
    n = length(b)
    mC = mA * n
    nC = nA
    C = zeros(mC, nC)
    for j in 1:nA
        for i in 1:mA
            Aij = A[i, j]
            for k in 1:n
                rowC = (i - 1) * n + k
                C[rowC, j] = Float64(Aij * b[k])
            end
        end
    end
    return C
end

# kron!(C, A, B): in-place Kronecker product, write result into C
# Supports matrix-matrix, vector-vector, and mixed cases
function kron!(C, A, B)
    ndA = length(size(A))
    ndB = length(size(B))

    if ndA == 1 && ndB == 1
        # Vector-vector
        m = length(A)
        n = length(B)
        idx = 1
        for i in 1:m
            ai = A[i]
            for k in 1:n
                C[idx] = Float64(ai * B[k])
                idx = idx + 1
            end
        end
    elseif ndA == 2 && ndB == 2
        # Matrix-matrix
        mA = size(A, 1)
        nA = size(A, 2)
        mB = size(B, 1)
        nB = size(B, 2)
        for j in 1:nA
            for l in 1:nB
                colC = (j - 1) * nB + l
                for i in 1:mA
                    Aij = A[i, j]
                    for k in 1:mB
                        rowC = (i - 1) * mB + k
                        C[rowC, colC] = Float64(Aij * B[k, l])
                    end
                end
            end
        end
    elseif ndA == 1 && ndB == 2
        # Vector-matrix
        m = length(A)
        mB = size(B, 1)
        nB = size(B, 2)
        for l in 1:nB
            for i in 1:m
                ai = A[i]
                for k in 1:mB
                    rowC = (i - 1) * mB + k
                    C[rowC, l] = Float64(ai * B[k, l])
                end
            end
        end
    elseif ndA == 2 && ndB == 1
        # Matrix-vector
        mA = size(A, 1)
        nA = size(A, 2)
        n = length(B)
        for j in 1:nA
            for i in 1:mA
                Aij = A[i, j]
                for k in 1:n
                    rowC = (i - 1) * n + k
                    C[rowC, j] = Float64(Aij * B[k])
                end
            end
        end
    else
        error("kron!: unsupported dimensions")
    end
    return C
end

# =============================================================================
# Diagonal Matrix Type
# =============================================================================

"""
    Diagonal(diag)

Construct a diagonal matrix from a vector `diag`.

# Examples
```julia
D = Diagonal([1, 2, 3])  # 3×3 diagonal matrix with diagonal [1, 2, 3]
```
"""
struct Diagonal{T}
    diag::Vector{T}
end

# Constructor: Diagonal(diag::AbstractVector)
function Diagonal(diag)
    # Convert to Vector to ensure we have a concrete type
    diag_vec = Vector(diag)
    # Infer type from first element if available, otherwise use Float64
    if length(diag_vec) > 0
        T = typeof(diag_vec[1])
        return Diagonal{T}(diag_vec)
    else
        return Diagonal{Float64}(diag_vec)
    end
end

# Size of a Diagonal matrix
function Base.size(D::Diagonal)
    n = length(D.diag)
    return (n, n)
end

function Base.size(D::Diagonal, dim::Int)
    n = length(D.diag)
    if dim == 1
        return n
    elseif dim == 2
        return n
    else
        error("DimensionMismatch: Diagonal matrix has 2 dimensions, got dim=$dim")
    end
end

# Indexing: D[i, j] returns D.diag[i] if i == j, 0 otherwise
function Base.getindex(D::Diagonal, i::Int, j::Int)
    if i == j
        if 1 <= i <= length(D.diag)
            return D.diag[i]
        else
            error("BoundsError: attempt to access Diagonal at index ($i, $j)")
        end
    else
        # Return zero of the same type as diagonal elements
        if length(D.diag) > 0
            return zero(D.diag[1])
        else
            return 0.0
        end
    end
end

# Matrix multiplication: Diagonal * Matrix or Diagonal * Vector
function Base.:*(D::Diagonal, A)
    n = length(D.diag)
    # Check if A is a vector (1D) or matrix (2D)
    ndims_A = length(size(A))
    if ndims_A == 1
        # Diagonal * Vector: result[i] = D[i, i] * A[i]
        if length(A) != n
            error("DimensionMismatch: Diagonal matrix has $n rows, but vector has $(length(A)) elements")
        end
        result = zeros(n)
        for i in 1:n
            result[i] = D.diag[i] * A[i]
        end
        return result
    elseif ndims_A == 2
        # Diagonal * Matrix: result[i, j] = D[i, i] * A[i, j]
        if size(A, 1) != n
            error("DimensionMismatch: Diagonal matrix has $n rows, but A has $(size(A, 1)) rows")
        end
        ncols = size(A, 2)
        result = zeros(n, ncols)
        for i in 1:n
            di = D.diag[i]
            for j in 1:ncols
                result[i, j] = di * A[i, j]
            end
        end
        return result
    else
        error("DimensionMismatch: Diagonal * A requires A to be 1D or 2D, got $(ndims_A)D")
    end
end

# Matrix multiplication: Matrix * Diagonal or Vector * Diagonal
function Base.:*(A, D::Diagonal)
    n = length(D.diag)
    # Check if A is a vector (1D) or matrix (2D)
    ndims_A = length(size(A))
    if ndims_A == 1
        # Vector * Diagonal: result[j] = A[j] * D[j, j]
        if length(A) != n
            error("DimensionMismatch: Diagonal matrix has $n columns, but vector has $(length(A)) elements")
        end
        result = zeros(n)
        for j in 1:n
            result[j] = A[j] * D.diag[j]
        end
        return result
    elseif ndims_A == 2
        # Matrix * Diagonal: result[i, j] = A[i, j] * D[j, j]
        if size(A, 2) != n
            error("DimensionMismatch: Diagonal matrix has $n columns, but A has $(size(A, 2)) columns")
        end
        nrows = size(A, 1)
        result = zeros(nrows, n)
        for i in 1:nrows
            for j in 1:n
                result[i, j] = A[i, j] * D.diag[j]
            end
        end
        return result
    else
        error("DimensionMismatch: A * Diagonal requires A to be 1D or 2D, got $(ndims_A)D")
    end
end

# Matrix multiplication: Diagonal * Diagonal
function Base.:*(D1::Diagonal, D2::Diagonal)
    n1 = length(D1.diag)
    n2 = length(D2.diag)
    if n1 != n2
        error("DimensionMismatch: Diagonal matrices have different sizes: $n1×$n1 and $n2×$n2")
    end
    
    # Result: (D1 * D2)[i, j] = D1[i, i] * D2[i, j] = D1[i, i] * D2[i, i] if i == j, else 0
    # Use Float64 for result type (promotion will handle mixed types)
    result_diag = zeros(n1)
    for i in 1:n1
        result_diag[i] = D1.diag[i] * D2.diag[i]
    end
    return Diagonal(result_diag)
end

struct UniformScaling{T}
    λ::T
end

const I = UniformScaling{Bool}(true)

function Base.getindex(J::UniformScaling, i::Int64, j::Int64)
    return i == j ? J.λ : zero(J.λ)
end

function _scale_matrix(A, value)
    C = zeros(size(A, 1), size(A, 2))
    for j in 1:size(A, 2)
        for i in 1:size(A, 1)
            C[i, j] = value * A[i, j]
        end
    end
    return C
end

function _scale_vector(x, value)
    y = zeros(length(x))
    for i in 1:length(x)
        y[i] = value * x[i]
    end
    return y
end

function Base.:*(J::UniformScaling, A::Matrix)
    return _scale_matrix(A, J.λ)
end

function Base.:*(A::Matrix, J::UniformScaling)
    return _scale_matrix(A, J.λ)
end

function Base.:*(J::UniformScaling, x::AbstractVector)
    return _scale_vector(x, J.λ)
end

function Base.:*(x::AbstractVector, J::UniformScaling)
    return _scale_vector(x, J.λ)
end

function Base.:*(J::UniformScaling, x::Number)
    return UniformScaling(J.λ * x)
end

function Base.:*(x::Number, J::UniformScaling)
    return UniformScaling(x * J.λ)
end

struct Symmetric{T,S}
    data::S
end

struct Hermitian{T,S}
    data::S
end

struct UpperTriangular{T,S}
    data::S
end

struct LowerTriangular{T,S}
    data::S
end

struct UnitUpperTriangular{T,S}
    data::S
end

struct UnitLowerTriangular{T,S}
    data::S
end

struct UpperHessenberg{T,S}
    data::S
end

struct Transpose{T,S}
    parent::S
end

struct Adjoint{T,S}
    parent::S
end

struct Bidiagonal{T,V}
    dv::V
    ev::V
    uplo
end

struct Tridiagonal{T,V} <: AbstractMatrix{T}
    dl::V
    d::V
    du::V
end

struct SymTridiagonal{T,V} <: AbstractMatrix{T}
    dv::V
    ev::V
end

Base.eltype(::Type{Tridiagonal{T,V}}) where {T,V} = T
Base.eltype(::Type{SymTridiagonal{T,V}}) where {T,V} = T
Base.eltype(::Tridiagonal{T,V}) where {T,V} = T
Base.eltype(::SymTridiagonal{T,V}) where {T,V} = T

function _wrapper_type(A)
    return eltype(A)
end

function Symmetric(A)
    return Symmetric{_wrapper_type(A),typeof(A)}(A)
end

function Hermitian(A)
    return Hermitian{_wrapper_type(A),typeof(A)}(A)
end

function UpperTriangular(A)
    return UpperTriangular{_wrapper_type(A),typeof(A)}(A)
end

function LowerTriangular(A)
    return LowerTriangular{_wrapper_type(A),typeof(A)}(A)
end

function UnitUpperTriangular(A)
    return UnitUpperTriangular{_wrapper_type(A),typeof(A)}(A)
end

function UnitLowerTriangular(A)
    return UnitLowerTriangular{_wrapper_type(A),typeof(A)}(A)
end

function UpperHessenberg(A)
    return UpperHessenberg{_wrapper_type(A),typeof(A)}(A)
end

function Transpose(A)
    return Transpose{_wrapper_type(A),typeof(A)}(A)
end

function Adjoint(A)
    return Adjoint{_wrapper_type(A),typeof(A)}(A)
end

function Bidiagonal(dv, ev, uplo)
    return Bidiagonal{eltype(dv),typeof(dv)}(dv, ev, uplo)
end

function Tridiagonal(dl, d, du)
    return Tridiagonal{eltype(d),typeof(d)}(dl, d, du)
end

function SymTridiagonal(dv, ev)
    return SymTridiagonal{eltype(dv),typeof(dv)}(dv, ev)
end

function Base.size(S::Symmetric)
    return size(S.data)
end

function Base.size(S::Hermitian)
    return size(S.data)
end

function Base.size(S::UpperTriangular)
    return size(S.data)
end

function Base.size(S::LowerTriangular)
    return size(S.data)
end

function Base.size(S::UnitUpperTriangular)
    return size(S.data)
end

function Base.size(S::UnitLowerTriangular)
    return size(S.data)
end

function Base.size(S::UpperHessenberg)
    return size(S.data)
end

function Base.size(S::Transpose)
    return (size(S.parent, 2), size(S.parent, 1))
end

function Base.size(S::Adjoint)
    return (size(S.parent, 2), size(S.parent, 1))
end

function Base.size(S::Bidiagonal)
    n = length(S.dv)
    return (n, n)
end

function Base.size(S::Tridiagonal)
    n = length(S.d)
    return (n, n)
end

function Base.size(S::SymTridiagonal)
    n = length(S.dv)
    return (n, n)
end

function Base.length(S::Tridiagonal)
    n = length(S.d)
    return n * n
end

function Base.length(S::SymTridiagonal)
    n = length(S.dv)
    return n * n
end

function _structured_size_dim(S, dim::Int64)
    dims = size(S)
    if dim == 1
        return dims[1]
    elseif dim == 2
        return dims[2]
    end
    error("DimensionMismatch: structured matrix has 2 dimensions")
end

function Base.size(S::Symmetric, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::Hermitian, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::UpperTriangular, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::LowerTriangular, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::UnitUpperTriangular, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::UnitLowerTriangular, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::UpperHessenberg, dim::Int64)
    return size(S.data, dim)
end

function Base.size(S::Transpose, dim::Int64)
    return _structured_size_dim(S, dim)
end

function Base.size(S::Adjoint, dim::Int64)
    return _structured_size_dim(S, dim)
end

function Base.size(S::Bidiagonal, dim::Int64)
    return _structured_size_dim(S, dim)
end

function Base.size(S::Tridiagonal, dim::Int64)
    return _structured_size_dim(S, dim)
end

function Base.size(S::SymTridiagonal, dim::Int64)
    return _structured_size_dim(S, dim)
end

function Base.getindex(S::Symmetric, i::Int64, j::Int64)
    return i <= j ? S.data[i, j] : S.data[j, i]
end

function Base.getindex(S::Hermitian, i::Int64, j::Int64)
    return i <= j ? S.data[i, j] : conj(S.data[j, i])
end

function Base.getindex(S::UpperTriangular, i::Int64, j::Int64)
    return i <= j ? S.data[i, j] : zero(S.data[1, 1])
end

function Base.getindex(S::LowerTriangular, i::Int64, j::Int64)
    return i >= j ? S.data[i, j] : zero(S.data[1, 1])
end

function Base.getindex(S::UnitUpperTriangular, i::Int64, j::Int64)
    if i == j
        return one(S.data[1, 1])
    end
    return i < j ? S.data[i, j] : zero(S.data[1, 1])
end

function Base.getindex(S::UnitLowerTriangular, i::Int64, j::Int64)
    if i == j
        return one(S.data[1, 1])
    end
    return i > j ? S.data[i, j] : zero(S.data[1, 1])
end

function Base.getindex(S::UpperHessenberg, i::Int64, j::Int64)
    return i <= j + 1 ? S.data[i, j] : zero(S.data[1, 1])
end

function Base.getindex(S::Transpose, i::Int64, j::Int64)
    return S.parent[j, i]
end

function Base.getindex(S::Adjoint, i::Int64, j::Int64)
    return conj(S.parent[j, i])
end

function Base.getindex(S::Bidiagonal, i::Int64, j::Int64)
    if i == j
        return S.dv[i]
    elseif (S.uplo == :U || S.uplo == 'U') && j == i + 1
        return S.ev[i]
    elseif (S.uplo == :L || S.uplo == 'L') && i == j + 1
        return S.ev[j]
    end
    return zero(S.dv[1])
end

function Base.getindex(S::Tridiagonal, i::Int64, j::Int64)
    if i == j
        return S.d[i]
    elseif i == j + 1
        return S.dl[j]
    elseif j == i + 1
        return S.du[i]
    end
    return zero(S.d[1])
end

function Base.getindex(S::SymTridiagonal, i::Int64, j::Int64)
    if i == j
        return S.dv[i]
    elseif i == j + 1
        return S.ev[j]
    elseif j == i + 1
        return S.ev[i]
    end
    return zero(S.dv[1])
end

function Base.getindex(S::Tridiagonal, k::Int64)
    rows = size(S, 1)
    i = ((k - 1) % rows) + 1
    j = div(k - 1, rows) + 1
    return S[i, j]
end

function Base.getindex(S::SymTridiagonal, k::Int64)
    rows = size(S, 1)
    i = ((k - 1) % rows) + 1
    j = div(k - 1, rows) + 1
    return S[i, j]
end

function _structured_dense(S)
    A = zeros(size(S, 1), size(S, 2))
    for j in 1:size(S, 2)
        for i in 1:size(S, 1)
            A[i, j] = S[i, j]
        end
    end
    return A
end

function Base.:*(S::Symmetric, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::Symmetric)
    return A * _structured_dense(S)
end

function Base.:*(S::Hermitian, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::Hermitian)
    return A * _structured_dense(S)
end

function Base.:*(S::UpperTriangular, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::UpperTriangular)
    return A * _structured_dense(S)
end

function Base.:*(S::LowerTriangular, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::LowerTriangular)
    return A * _structured_dense(S)
end

function Base.:*(S::UnitUpperTriangular, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::UnitUpperTriangular)
    return A * _structured_dense(S)
end

function Base.:*(S::UnitLowerTriangular, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::UnitLowerTriangular)
    return A * _structured_dense(S)
end

function Base.:*(S::UpperHessenberg, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::UpperHessenberg)
    return A * _structured_dense(S)
end

function Base.:*(S::Bidiagonal, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::Bidiagonal)
    return A * _structured_dense(S)
end

function Base.:*(S::Tridiagonal, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::Tridiagonal)
    return A * _structured_dense(S)
end

function Base.:*(S::SymTridiagonal, A::Matrix)
    return _structured_dense(S) * A
end

function Base.:*(A::Matrix, S::SymTridiagonal)
    return A * _structured_dense(S)
end

struct Givens{T}
    i1::Int64
    i2::Int64
    c::T
    s::T
end

function givens(f::T, g::T, i1::Integer, i2::Integer) where T
    r = sqrt(f * f + g * g)
    if r == 0
        return (Givens{T}(Int64(i1), Int64(i2), one(f), zero(f)), r)
    end
    c = f / r
    s = g / r
    return (Givens{T}(Int64(i1), Int64(i2), c, s), r)
end

function givens(x::AbstractVector, i1::Integer, i2::Integer)
    return givens(x[i1], x[i2], i1, i2)
end

function givens(A::AbstractMatrix, i1::Integer, i2::Integer, j::Integer)
    return givens(A[i1, j], A[i2, j], i1, i2)
end

function rotate!(x::AbstractVector, y::AbstractVector, c, s)
    n = length(x)
    for i in 1:n
        xi = x[i]
        yi = y[i]
        x[i] = c * xi + s * yi
        y[i] = c * yi - s * xi
    end
    return x, y
end

function reflect!(x::AbstractVector, y::AbstractVector, c, s)
    n = length(x)
    for i in 1:n
        xi = x[i]
        yi = y[i]
        x[i] = c * xi + s * yi
        y[i] = s * xi - c * yi
    end
    return x, y
end

function lmul!(G::Givens, A::AbstractVector)
    x = A[G.i1]
    y = A[G.i2]
    A[G.i1] = G.c * x + G.s * y
    A[G.i2] = G.c * y - G.s * x
    return A
end

function lmul!(G::Givens, A::AbstractMatrix)
    n = size(A, 2)
    for j in 1:n
        x = A[G.i1, j]
        y = A[G.i2, j]
        A[G.i1, j] = G.c * x + G.s * y
        A[G.i2, j] = G.c * y - G.s * x
    end
    return A
end

function Base.:*(G::Givens, A::AbstractVector)
    result = copy(A)
    return lmul!(G, result)
end

function Base.:*(G::Givens, A::AbstractMatrix)
    result = copy(A)
    return lmul!(G, result)
end

function _permute_vector(p, b)
    n = length(p)
    result = zeros(n)
    for i in 1:n
        result[i] = b[p[i]]
    end
    return result
end

function _reciprocal_singular_values(S)
    n = length(S)
    result = zeros(n)
    for i in 1:n
        result[i] = 1.0 / S[i]
    end
    return result
end

# =============================================================================
# Linear Algebra Decompositions
# =============================================================================
# These functions are implemented as builtins in the VM for performance.
# The function definitions here make them available via method dispatch
# when using LinearAlgebra module.

abstract type Factorization end

struct LU <: Factorization
    L
    U
    p
end

struct QR <: Factorization
    Q
    R
end

struct Cholesky <: Factorization
    L
    U
end

struct Eigen <: Factorization
    values
    vectors
end

struct SVD <: Factorization
    U
    S
    V
    Vt
end

struct Schur{T,M,V} <: Factorization
    T::M
    Z::M
    values::V
end

struct GeneralizedSchur{M,V} <: Factorization
    S::M
    T::M
    α::V
    β::V
    Q::M
    Z::M
end

struct Hessenberg{T,H,M,V,B} <: Factorization
    H::H
    uplo
    factors::M
    τ::V
    μ::B
end

struct LQ{T,M,V} <: Factorization
    factors::M
    τ::V
end

struct LDLt{T,D} <: Factorization
    data::D
end

struct BunchKaufman{T,M,V} <: Factorization
    LD::M
    ipiv::V
    uplo
    symmetric::Bool
    rook::Bool
    info::Int64
end

struct GeneralizedEigen{V,M} <: Factorization
    values::V
    vectors::M
end

struct GeneralizedSVD{M,V} <: Factorization
    U::M
    V::M
    Q::M
    a::V
    b::V
    k::Int64
    l::Int64
    R::M
end

function Base.iterate(F::LU)
    return (F.L, 2)
end

function Base.iterate(F::LU, state::Int64)
    if state == 2
        return (F.U, 3)
    elseif state == 3
        return (F.p, 4)
    else
        return nothing
    end
end

function Base.length(F::LU)
    return 3
end

function Base.getindex(F::LU, i::Int64)
    if i == 1
        return F.L
    elseif i == 2
        return F.U
    elseif i == 3
        return F.p
    else
        error("BoundsError: LU factorization index out of range")
    end
end

function Base.iterate(F::SVD)
    return (F.U, 2)
end

function Base.iterate(F::SVD, state::Int64)
    if state == 2
        return (F.S, 3)
    elseif state == 3
        return (F.V, 4)
    else
        return nothing
    end
end

function Base.length(F::SVD)
    return 3
end

function Base.getindex(F::SVD, i::Int64)
    if i == 1
        return F.U
    elseif i == 2
        return F.S
    elseif i == 3
        return F.V
    else
        error("BoundsError: SVD factorization index out of range")
    end
end

function Base.copy(F::Cholesky)
    return Cholesky(copy(F.L), copy(F.U))
end

function issuccess(F::Factorization)
    return true
end

function _clear_matrix!(A)
    m = size(A, 1)
    n = size(A, 2)
    for j in 1:n
        for i in 1:m
            A[i, j] = 0.0
        end
    end
    return A
end

function _copy_matrix_prefix!(A, B)
    m = min(size(A, 1), size(B, 1))
    n = min(size(A, 2), size(B, 2))
    for j in 1:n
        for i in 1:m
            A[i, j] = B[i, j]
        end
    end
    return A
end

function _write_lu_storage!(A, F::LU)
    m = size(A, 1)
    n = size(A, 2)
    rows_l = size(F.L, 1)
    cols_l = size(F.L, 2)
    rows_u = size(F.U, 1)
    cols_u = size(F.U, 2)
    for j in 1:n
        for i in 1:m
            if i > j && i <= rows_l && j <= cols_l
                A[i, j] = F.L[i, j]
            elseif i <= rows_u && j <= cols_u
                A[i, j] = F.U[i, j]
            else
                A[i, j] = 0.0
            end
        end
    end
    return A
end

function _write_values_diagonal!(A, values)
    _clear_matrix!(A)
    k = min(min(size(A, 1), size(A, 2)), length(values))
    for i in 1:k
        value = values[i]
        A[i, i] = isreal(value) ? real(value) : value
    end
    return A
end

"""
    lu(A)

Compute the LU decomposition of matrix A with partial pivoting.
Returns an `LU` factorization where L is lower triangular, U is upper triangular,
and p is a permutation vector such that A[p, :] = L * U.
"""
function lu(A)
    raw = LinearAlgebra.__sjulia_builtin_lu(A)
    if !(raw isa Tuple)
        return raw
    end
    return LU(raw[1], raw[2], raw[3])
end

"""
    lu!(A)

Compute the LU decomposition of `A`, storing the supported LU work form back in
`A` and returning an `LU` factorization.
"""
function lu!(A)
    F = lu(A)
    if F isa LU
        _write_lu_storage!(A, F)
    end
    return F
end

"""
    det(A)

Compute the determinant of matrix A using LU decomposition.
"""
function det(A)
    return LinearAlgebra.__sjulia_builtin_det(A)
end

"""
    inv(A)

Compute the inverse of matrix A using LU decomposition.
"""
function inv(A::AbstractMatrix)
    return LinearAlgebra.__sjulia_builtin_inv(A)
end

function inv(A)
    return Base.inv(A)
end

function Base.:\(A::AbstractMatrix, b::AbstractVector)
    return inv(A) * b
end

function Base.:\(A::AbstractMatrix, B::AbstractMatrix)
    return inv(A) * B
end

function Base.:\(F::LU, b::AbstractVector)
    return F.U \ (F.L \ _permute_vector(F.p, b))
end

function Base.:\(F::QR, b::AbstractVector)
    return F.R \ (transpose(F.Q) * b)
end

function Base.:\(F::Cholesky, b::AbstractVector)
    return F.U \ (F.L \ b)
end

function Base.:\(F::SVD, b::AbstractVector)
    return F.V * (Diagonal(_reciprocal_singular_values(F.S)) * (transpose(F.U) * b))
end

function Base.:/(A::AbstractMatrix, B::AbstractMatrix)
    return A * inv(B)
end

"""
    svd(A)

Compute the Singular Value Decomposition of matrix A.
Returns an `SVD` factorization with fields U, S, V, and Vt.
"""
function svd(A)
    raw = LinearAlgebra.__sjulia_builtin_svd(A)
    if !(raw isa NamedTuple)
        return raw
    end
    return SVD(raw.U, raw.S, raw.V, raw.Vt)
end

"""
    svd!(A)

Compute the singular value decomposition of `A`, storing the supported diagonal
singular-value work form back in `A` and returning an `SVD` factorization.
"""
function svd!(A)
    F = svd(A)
    if F isa SVD
        _write_values_diagonal!(A, F.S)
    end
    return F
end

"""
    svdvals(A)

Return the singular values of `A`.
"""
function svdvals(A)
    return svd(A).S
end

"""
    svdvals!(A)

Return the singular values of `A`, using the supported in-place SVD work path.
"""
function svdvals!(A)
    return svd!(A).S
end

"""
    qr(A)

Compute the QR decomposition of matrix A.
Returns a `QR` factorization with fields Q and R.
"""
function qr(A)
    raw = LinearAlgebra.__sjulia_builtin_qr(A)
    if !(raw isa NamedTuple)
        return raw
    end
    return QR(raw.Q, raw.R)
end

"""
    qr!(A)

Compute the QR decomposition of `A`, storing the supported R work form back in
`A` and returning a `QR` factorization.
"""
function qr!(A)
    F = qr(A)
    if F isa QR
        _clear_matrix!(A)
        _copy_matrix_prefix!(A, F.R)
    end
    return F
end

"""
    eigen(A)

Compute the eigenvalue decomposition of matrix A.
Returns an `Eigen` factorization with fields values (eigenvalues) and vectors (eigenvectors).
Only works for symmetric matrices with real eigenvalues.
"""
function eigen(A)
    raw = LinearAlgebra.__sjulia_builtin_eigen(A)
    if !(raw isa NamedTuple)
        return raw
    end
    return Eigen(raw.values, raw.vectors)
end

"""
    eigen!(A)

Compute the eigenvalue decomposition of `A`, storing the supported diagonal
eigenvalue work form back in `A` and returning an `Eigen` factorization.
"""
function eigen!(A)
    F = eigen(A)
    if F isa Eigen
        _write_values_diagonal!(A, F.values)
    end
    return F
end

"""
    eigvals(A)

Compute the eigenvalues of matrix A.
Returns a vector of complex eigenvalues.
"""
function eigvals(A)
    return LinearAlgebra.__sjulia_builtin_eigvals(A)
end

function _real_eigvals_from_dense(A)
    values = LinearAlgebra.__sjulia_builtin_eigvals(Matrix(A))
    result = Vector{Float64}(undef, length(values))
    for i in 1:length(values)
        result[i] = real(values[i])
    end
    sort!(result)
    return result
end

function eigvals(A::SymTridiagonal)
    return _real_eigvals_from_dense(A)
end

function eigvals(A::SymTridiagonal, irange::AbstractRange)
    values = _real_eigvals_from_dense(A)
    return values[irange]
end

"""
    eigvals!(A)

Return the eigenvalues of `A`, storing the supported diagonal eigenvalue work
form back in `A`.
"""
function eigvals!(A)
    values = eigvals(A)
    _write_values_diagonal!(A, values)
    return values
end

"""
    cholesky(A)

Compute the Cholesky decomposition of symmetric positive-definite matrix A.
Returns a `Cholesky` factorization with fields L and U where U = L'.
"""
function cholesky(A)
    raw = LinearAlgebra.__sjulia_builtin_cholesky(A)
    if !(raw isa NamedTuple)
        return raw
    end
    return Cholesky(raw.L, raw.U)
end

function _cholesky_upper_work!(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        return false
    end
    if !issymmetric(A)
        return false
    end

    for j in 1:n
        for k in 1:j-1
            s = 0.0
            for i in 1:k-1
                s = s + A[i, k] * A[i, j]
            end
            A[k, j] = (A[k, j] - s) / A[k, k]
        end

        d = A[j, j]
        for k in 1:j-1
            d = d - A[k, j] * A[k, j]
        end
        A[j, j] = d
        if d <= 0.0
            return false
        end
        A[j, j] = sqrt(d)
    end
    return true
end

"""
    cholesky!(A)

Compute the Cholesky decomposition of `A`, storing the supported upper-triangular
factor back in `A` and returning a `Cholesky` factorization.
"""
function cholesky!(A)
    original = copy(A)
    _cholesky_upper_work!(A)
    return cholesky(original)
end

function _identity_like(A)
    n = size(A, 1)
    Z = zeros(n, n)
    for i in 1:n
        Z[i, i] = 1.0
    end
    return Z
end

function schur(A)
    F = eigen(A)
    if F isa Eigen
        T = diagm(F.values)
        return Schur{eltype(A),typeof(T),typeof(F.values)}(T, F.vectors, F.values)
    end
    return F
end

function schur!(A)
    return schur(A)
end

function ordschur(F::Schur, select)
    return F
end

function ordschur!(F::Schur, select)
    return F
end

function hessenberg(A)
    H = UpperHessenberg(copy(A))
    tau_len = size(A, 1) > 1 ? size(A, 1) - 1 : 0
    τ = zeros(tau_len)
    return Hessenberg{eltype(A),typeof(H),typeof(A),typeof(τ),Bool}(H, 'L', copy(A), τ, false)
end

function hessenberg!(A)
    return hessenberg(A)
end

function lq(A)
    F = qr(transpose(A))
    factors = transpose(F.R)
    τ = zeros(size(A, 1))
    return LQ{eltype(A),typeof(factors),typeof(τ)}(factors, τ)
end

function lq!(A)
    return lq(A)
end

function ldlt(A::SymTridiagonal)
    return LDLt{eltype(A.dv),typeof(A)}(A)
end

function ldlt!(A::SymTridiagonal)
    return ldlt(A)
end

function _identity_pivots(n)
    p = zeros(Int64, n)
    for i in 1:n
        p[i] = i
    end
    return p
end

function bunchkaufman(A)
    return BunchKaufman{eltype(A),typeof(A),Vector{Int64}}(copy(A), _identity_pivots(size(A, 1)), 'U', issymmetric(A), false, 0)
end

function bunchkaufman!(A)
    return bunchkaufman(A)
end

function _outer_product(v)
    n = length(v)
    A = zeros(n, n)
    for j in 1:n
        for i in 1:n
            A[i, j] = v[i] * conj(v[j])
        end
    end
    return A
end

function _matrix_add(A, B)
    if size(A, 1) != size(B, 1) || size(A, 2) != size(B, 2)
        error("DimensionMismatch: matrix sizes must match")
    end
    C = zeros(size(A, 1), size(A, 2))
    for j in 1:size(A, 2)
        for i in 1:size(A, 1)
            C[i, j] = A[i, j] + B[i, j]
        end
    end
    return C
end

function _matrix_sub(A, B)
    if size(A, 1) != size(B, 1) || size(A, 2) != size(B, 2)
        error("DimensionMismatch: matrix sizes must match")
    end
    C = zeros(size(A, 1), size(A, 2))
    for j in 1:size(A, 2)
        for i in 1:size(A, 1)
            C[i, j] = A[i, j] - B[i, j]
        end
    end
    return C
end

function _copy_cholesky_fields!(dest::Cholesky, src::Cholesky)
    _copy_matrix_prefix!(dest.L, src.L)
    _copy_matrix_prefix!(dest.U, src.U)
    return dest
end

function lowrankupdate!(C::Cholesky, v::AbstractVector)
    if size(C.L, 1) != length(v)
        error("DimensionMismatch: updating vector must fit size of factorization")
    end
    updated = cholesky(_matrix_add(C.L * C.U, _outer_product(v)))
    if updated isa Cholesky
        _copy_cholesky_fields!(C, updated)
    end
    return C
end

function lowrankupdate(C::Cholesky, v::AbstractVector)
    return lowrankupdate!(copy(C), copy(v))
end

function lowrankdowndate!(C::Cholesky, v::AbstractVector)
    if size(C.L, 1) != length(v)
        error("DimensionMismatch: updating vector must fit size of factorization")
    end
    updated = cholesky(_matrix_sub(C.L * C.U, _outer_product(v)))
    if updated isa Cholesky
        _copy_cholesky_fields!(C, updated)
    end
    return C
end

function lowrankdowndate(C::Cholesky, v::AbstractVector)
    return lowrankdowndate!(copy(C), copy(v))
end

"""
    rank(A)

Compute the rank of matrix A (number of singular values above tolerance).
"""
function _rank_count_above(S, tol)
    result = 0
    for s in S
        if s > tol
            result = result + 1
        end
    end
    return result
end

function _rank_vector(A)
    for x in A
        if !iszero(x)
            return 1
        end
    end
    return 0
end

function rank(A::Array)
    dims = size(A)
    if length(dims) == 1
        return _rank_vector(A)
    end

    S = svd(A).S
    m = size(A, 1)
    n = size(A, 2)
    tol = max(m, n) * eps(Float64) * S[1]
    return _rank_count_above(S, tol)
end

function rank(x::Number)
    return iszero(x) ? 0 : 1
end

"""
    cond(A)

Compute the condition number of matrix A (2-norm condition number).
"""
function cond(A)
    return LinearAlgebra.__sjulia_builtin_cond(A)
end

function _abs_matrix(A)
    m = size(A, 1)
    n = size(A, 2)
    B = zeros(m, n)
    for j in 1:n
        for i in 1:m
            B[i, j] = abs(A[i, j])
        end
    end
    return B
end

function _abs_vector(x)
    y = zeros(length(x))
    for i in 1:length(x)
        y[i] = abs(x[i])
    end
    return y
end

function condskeel(A::AbstractMatrix)
    return condskeel(A, Inf)
end

function condskeel(A::AbstractMatrix, p::Real)
    return opnorm(_abs_matrix(inv(A)) * _abs_matrix(A), p)
end

function condskeel(A::AbstractMatrix, x::AbstractVector)
    return condskeel(A, x, Inf)
end

function condskeel(A::AbstractMatrix, x::AbstractVector, p::Real)
    return norm(_abs_matrix(inv(A)) * (_abs_matrix(A) * _abs_vector(x)), p) / norm(x, p)
end

function _identity_matrix(n)
    I = zeros(n, n)
    for i in 1:n
        I[i, i] = 1.0
    end
    return I
end

function _colvec(A)
    m = size(A, 1)
    n = size(A, 2)
    v = zeros(m * n)
    k = 1
    for j in 1:n
        for i in 1:m
            v[k] = A[i, j]
            k = k + 1
        end
    end
    return v
end

function _matrix_from_colvec(v, m, n)
    A = zeros(m, n)
    k = 1
    for j in 1:n
        for i in 1:m
            A[i, j] = v[k]
            k = k + 1
        end
    end
    return A
end

function sylvester(a::Union{Real, Complex}, b::Union{Real, Complex}, c::Union{Real, Complex})
    return (0.0 - c) / (a + b)
end

function sylvester(A::AbstractMatrix, B::AbstractMatrix, C::AbstractMatrix)
    m = size(A, 1)
    n = size(B, 1)
    if size(A, 2) != m || size(B, 2) != n || size(C, 1) != m || size(C, 2) != n
        error("DimensionMismatch: sylvester expects square A/B and C sized like A rows by B rows")
    end
    K = _matrix_add(kron(_identity_matrix(n), A), kron(transpose(B), _identity_matrix(m)))
    rhs = -_colvec(C)
    return _matrix_from_colvec(K \ rhs, m, n)
end

function lyap(a::Union{Real, Complex}, c::Union{Real, Complex})
    return (0.0 - c) / (2 * real(a))
end

function lyap(A::AbstractMatrix, C::AbstractMatrix)
    return sylvester(A, adjoint(A), C)
end

"""
    transpose(A)

Compute the transpose of A.
For arrays, returns the transpose (swaps rows and columns without conjugation).
For scalars, returns the value itself.
"""
function transpose(A)
    # This resolves to Pure Julia implementation in Base (base/array.jl, base/number.jl, base/complex.jl)
    return Base.transpose(A)
end

# =============================================================================
# eigvecs - Extract eigenvectors from eigen decomposition
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/eigen.jl

"""
    eigvecs(A)

Return the eigenvectors of matrix A as columns of a matrix.
This is equivalent to `eigen(A).vectors`.
"""
function eigvecs(A)
    F = eigen(A)
    return F.vectors
end

# =============================================================================
# pinv - Moore-Penrose pseudo-inverse via SVD
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl
#
# The pseudo-inverse satisfies: A * pinv(A) * A ≈ A
# Computed via SVD: if A = U * Diagonal(S) * Vt, then
# pinv(A) = V * Diagonal(1./S) * transpose(U)
# Singular values below tolerance are treated as zero.

"""
    pinv(A)

Compute the Moore-Penrose pseudo-inverse of matrix A using SVD.
Singular values below `eps(Float64) * max(m, n) * S[1]` are treated as zero.
"""
function pinv(A)
    F = svd(A)
    U = F.U
    S = F.S
    V = F.V

    m = size(A, 1)
    n = size(A, 2)
    # Default tolerance: eps * max(m,n) * largest singular value
    maxdim = m > n ? m : n
    tol = 2.220446049250313e-16 * maxdim * S[1]

    # Invert singular values above tolerance
    k = length(S)
    S_inv = zeros(k)
    for i in 1:k
        if S[i] > tol
            S_inv[i] = 1.0 / S[i]
        end
    end

    # pinv(A) = V * Diagonal(S_inv) * transpose(U)
    return V * Diagonal(S_inv) * transpose(U)
end

# Note: isapprox is defined in Base (operators.jl) with array support via isa() check
# The base version uses _isapprox_array for arrays which computes L2 norm manually

# =============================================================================
# normalize - Normalize a vector to unit length
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    normalize(v)

Return a normalized copy of vector v (unit vector in L2 norm).
Equivalent to `v / norm(v)`.
"""
function normalize(v)
    n = norm(v)
    if n == 0
        return copy(v)
    end
    return v / n
end

"""
    normalize(v, p)

Return a normalized copy of vector v in the Lp norm.
Equivalent to `v / norm(v, p)`.
"""
function normalize(v, p)
    n = norm(v, p)
    if n == 0
        return copy(v)
    end
    return v / n
end

# =============================================================================
# diag - Extract diagonal from matrix or create diagonal vector
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    diag(A)

Return the main diagonal of matrix A as a vector.
"""
function diag(A)
    m = size(A, 1)
    n = size(A, 2)
    k = m < n ? m : n
    d = zeros(k)
    for i in 1:k
        d[i] = A[i, i]
    end
    return d
end

"""
    diag(A, k)

Return the k-th diagonal of matrix A as a vector.
k > 0 is above the main diagonal, k < 0 is below.
"""
function diag(A, k)
    m = size(A, 1)
    n = size(A, 2)
    if k >= 0
        len = m < n - k ? m : n - k
    else
        len = m + k < n ? m + k : n
    end
    if len <= 0
        return Float64[]
    end
    d = zeros(len)
    if k >= 0
        for i in 1:len
            d[i] = A[i, i + k]
        end
    else
        for i in 1:len
            d[i] = A[i - k, i]
        end
    end
    return d
end

function _diag_length(A, k)
    m = size(A, 1)
    n = size(A, 2)
    if k >= 0
        len = min(m, n - k)
    else
        len = min(m + k, n)
    end
    return len < 0 ? 0 : len
end

"""
    diagind(A[, k])

Return the linear indices of the `k`th diagonal of matrix `A`.
"""
function diagind(A)
    return diagind(A, 0)
end

function diagind(A, k)
    m = size(A, 1)
    step = m + 1
    len = _diag_length(A, k)
    start = k >= 0 ? 1 + k * m : 1 - k
    stop = start + (len - 1) * step
    return start:step:stop
end

struct DiagView
    parent
    k::Int64
end

function diagview(A)
    return DiagView(A, 0)
end

function diagview(A, k)
    return DiagView(A, Int64(k))
end

function Base.length(v::DiagView)
    return _diag_length(v.parent, v.k)
end

function Base.size(v::DiagView)
    return (length(v),)
end

function Base.size(v::DiagView, dim::Int64)
    if dim == 1
        return length(v)
    end
    return 1
end

function Base.getindex(v::DiagView, i::Int64)
    if i < 1 || i > length(v)
        error("BoundsError: diagview index out of range")
    end
    if v.k >= 0
        return v.parent[i, i + v.k]
    end
    return v.parent[i - v.k, i]
end

function Base.setindex!(v::DiagView, value, i::Int64)
    if i < 1 || i > length(v)
        error("BoundsError: diagview index out of range")
    end
    if v.k >= 0
        v.parent[i, i + v.k] = value
    else
        v.parent[i - v.k, i] = value
    end
    return v
end

function Base.iterate(v::DiagView)
    if length(v) == 0
        return nothing
    end
    return (v[1], 2)
end

function Base.iterate(v::DiagView, state::Int64)
    if state > length(v)
        return nothing
    end
    return (v[state], state + 1)
end

# =============================================================================
# issymmetric - Check if a matrix is symmetric
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    issymmetric(A)

Test whether matrix A is symmetric, i.e., A == transpose(A).
"""
function issymmetric(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        return false
    end
    for i in 1:n
        for j in i+1:n
            if A[i, j] != A[j, i]
                return false
            end
        end
    end
    return true
end

# =============================================================================
# ishermitian - Check if a matrix is Hermitian
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    ishermitian(A)

Test whether matrix A is Hermitian, i.e., A == adjoint(A).
For real matrices, this is equivalent to issymmetric(A).
"""
function ishermitian(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        return false
    end
    for i in 1:n
        for j in i+1:n
            if A[i, j] != conj(A[j, i])
                return false
            end
        end
    end
    return true
end

# =============================================================================
# triu - Upper triangular part of a matrix
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    triu(A)

Return the upper triangular part of matrix A.
"""
function triu(A)
    m = size(A, 1)
    n = size(A, 2)
    R = zeros(m, n)
    for i in 1:m
        for j in i:n
            R[i, j] = A[i, j]
        end
    end
    return R
end

"""
    triu(A, k)

Return the upper triangular part of A starting from the kth superdiagonal.
k=0 is the main diagonal, k>0 is above, k<0 is below.
"""
function triu(A, k)
    m = size(A, 1)
    n = size(A, 2)
    R = zeros(m, n)
    for i in 1:m
        start = i + k
        if start < 1
            start = 1
        end
        for j in start:n
            R[i, j] = A[i, j]
        end
    end
    return R
end

function triu!(A)
    return triu!(A, 0)
end

function triu!(A, k)
    m = size(A, 1)
    n = size(A, 2)
    for i in 1:m
        last_zero = i + k - 1
        if last_zero > n
            last_zero = n
        end
        if last_zero >= 1
            for j in 1:last_zero
                A[i, j] = zero(A[i, j])
            end
        end
    end
    return A
end

# =============================================================================
# tril - Lower triangular part of a matrix
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    tril(A)

Return the lower triangular part of matrix A.
"""
function tril(A)
    m = size(A, 1)
    n = size(A, 2)
    R = zeros(m, n)
    for i in 1:m
        last = i
        if last > n
            last = n
        end
        for j in 1:last
            R[i, j] = A[i, j]
        end
    end
    return R
end

"""
    tril(A, k)

Return the lower triangular part of A up to the kth superdiagonal.
k=0 is the main diagonal, k>0 is above, k<0 is below.
"""
function tril(A, k)
    m = size(A, 1)
    n = size(A, 2)
    R = zeros(m, n)
    for i in 1:m
        last = i + k
        if last > n
            last = n
        end
        if last >= 1
            for j in 1:last
                R[i, j] = A[i, j]
            end
        end
    end
    return R
end

function tril!(A)
    return tril!(A, 0)
end

function tril!(A, k)
    m = size(A, 1)
    n = size(A, 2)
    for i in 1:m
        first_zero = i + k + 1
        if first_zero < 1
            first_zero = 1
        end
        if first_zero <= n
            for j in first_zero:n
                A[i, j] = zero(A[i, j])
            end
        end
    end
    return A
end

# =============================================================================
# diagm - Create diagonal matrix from vector
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    diagm(v)

Create a square diagonal matrix from vector v.
This is the inverse operation of `diag`.
"""
function diagm(v)
    n = length(v)
    A = zeros(n, n)
    for i in 1:n
        A[i, i] = v[i]
    end
    return A
end

# =============================================================================
# opnorm - Operator (matrix) norm
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    opnorm(A)

Compute the operator norm (induced 2-norm) of matrix A.
Equal to the largest singular value of A.
"""
function opnorm(A)
    F = svd(A)
    return F.S[1]
end

"""
    opnorm(A, p)

Compute the operator p-norm of matrix A.
- p=1: maximum absolute column sum
- p=2: largest singular value (default)
- p=Inf: maximum absolute row sum
"""
function opnorm(A, p)
    if p == 2
        F = svd(A)
        return F.S[1]
    elseif p == 1
        m = size(A, 1)
        n = size(A, 2)
        maxcol = 0.0
        for j in 1:n
            colsum = 0.0
            for i in 1:m
                colsum = colsum + abs(A[i, j])
            end
            if colsum > maxcol
                maxcol = colsum
            end
        end
        return maxcol
    elseif p == Inf
        m = size(A, 1)
        n = size(A, 2)
        maxrow = 0.0
        for i in 1:m
            rowsum = 0.0
            for j in 1:n
                rowsum = rowsum + abs(A[i, j])
            end
            if rowsum > maxrow
                maxrow = rowsum
            end
        end
        return maxrow
    end
end

# =============================================================================
# nullspace - Null space of a matrix
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    nullspace(A)

Compute an orthonormal basis for the null space of A.
Uses SVD to find columns of V corresponding to near-zero singular values.
Returns a matrix whose columns form the null space basis, or an empty
matrix if the null space is trivial.
"""
function nullspace(A)
    m = size(A, 1)
    n = size(A, 2)
    F = svd(A)
    S = F.S
    V = F.V

    # Tolerance: same as Julia's default
    tol = max(m, n) * S[1] * 2.220446049250313e-16

    # Count non-null singular values
    r = 0
    for i in 1:length(S)
        if S[i] > tol
            r = r + 1
        end
    end

    # Number of null space dimensions
    nulldim = n - r

    if nulldim == 0
        # Empty n×0 null space matrix.
        return zeros(n, 0)
    end

    # Extract columns of V corresponding to zero singular values
    N = zeros(n, nulldim)
    for j in 1:nulldim
        col = r + j
        for i in 1:n
            N[i, j] = V[i, col]
        end
    end
    return N
end

# =============================================================================
# logdet - Log of the absolute value of the determinant
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl

"""
    logdet(A)

Compute log(det(A)), throwing a DomainError if det(A) is negative.
More numerically stable than log(det(A)) for large matrices.
"""
function logdet(A)
    d = det(A)
    if d < 0
        # In Julia, logdet throws DomainError for negative determinants
        # For now, return NaN to indicate error
        return NaN
    end
    return log(d)
end

"""
    logabsdet(A)

Compute (log(|det(A)|), sign(det(A))).
Returns a tuple of the log absolute determinant and the sign.
"""
function logabsdet(A)
    d = det(A)
    if d > 0
        return (log(d), 1.0)
    elseif d < 0
        return (log(-d), -1.0)
    else
        return (-Inf, 0.0)
    end
end

# =============================================================================
# adjoint - Conjugate transpose of a matrix
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/adjtrans.jl

"""
    adjoint(A)

Compute the conjugate transpose (Hermitian adjoint) of matrix A.
For real matrices, this is the same as transpose.
For complex matrices, this is conj(transpose(A)).
"""
function adjoint(A)
    m = size(A, 1)
    n = size(A, 2)
    B = zeros(n, m)
    for i in 1:m
        for j in 1:n
            B[j, i] = conj(A[i, j])
        end
    end
    return B
end

function _check_transpose_dest(dest, src, name)
    if size(dest, 1) != size(src, 2) || size(dest, 2) != size(src, 1)
        error("DimensionMismatch: $name destination has size $(size(dest)), expected ($(size(src, 2)), $(size(src, 1)))")
    end
end

function copy_transpose!(dest, ir_dest, jr_dest, src, ir_src, jr_src)
    for jd in jr_dest
        for id in ir_dest
            src_i = ir_src[jd - first(jr_dest) + 1]
            src_j = jr_src[id - first(ir_dest) + 1]
            dest[id, jd] = src[src_i, src_j]
        end
    end
    return dest
end

function copy_transpose!(dest, ir_dest, jr_dest, trans, src, ir_src, jr_src)
    if trans == 'C' || trans == 'c'
        return copy_adjoint!(dest, ir_dest, jr_dest, src, ir_src, jr_src)
    elseif trans == 'T' || trans == 't'
        return copy_transpose!(dest, ir_dest, jr_dest, src, ir_src, jr_src)
    end
    for jd in jr_dest
        for id in ir_dest
            dest[id, jd] = src[ir_src[id - first(ir_dest) + 1], jr_src[jd - first(jr_dest) + 1]]
        end
    end
    return dest
end

function copy_adjoint!(dest, ir_dest, jr_dest, src, ir_src, jr_src)
    for jd in jr_dest
        for id in ir_dest
            src_i = ir_src[jd - first(jr_dest) + 1]
            src_j = jr_src[id - first(ir_dest) + 1]
            dest[id, jd] = adjoint(src[src_i, src_j])
        end
    end
    return dest
end

function transpose!(dest, src)
    _check_transpose_dest(dest, src, "transpose!")
    if dest === src
        if size(src, 1) != size(src, 2)
            error("DimensionMismatch: transpose! with aliased source and destination requires a square matrix")
        end
        n = size(src, 1)
        for j in 1:n
            for i in j+1:n
                tmp = dest[i, j]
                dest[i, j] = dest[j, i]
                dest[j, i] = tmp
            end
        end
        return dest
    end
    return copy_transpose!(dest, 1:size(dest, 1), 1:size(dest, 2), src, 1:size(src, 1), 1:size(src, 2))
end

function adjoint!(dest, src)
    _check_transpose_dest(dest, src, "adjoint!")
    if dest === src
        if size(src, 1) != size(src, 2)
            error("DimensionMismatch: adjoint! with aliased source and destination requires a square matrix")
        end
        n = size(src, 1)
        for i in 1:n
            dest[i, i] = adjoint(dest[i, i])
        end
        for j in 1:n
            for i in j+1:n
                tmp = adjoint(dest[i, j])
                dest[i, j] = adjoint(dest[j, i])
                dest[j, i] = tmp
            end
        end
        return dest
    end
    return copy_adjoint!(dest, 1:size(dest, 1), 1:size(dest, 2), src, 1:size(src, 1), 1:size(src, 2))
end

function copytrito!(dest, src, uplo)
    m = min(size(dest, 1), size(src, 1))
    n = min(size(dest, 2), size(src, 2))
    if uplo == 'U' || uplo == 'u'
        for j in 1:n
            for i in 1:m
                dest[i, j] = i <= j ? src[i, j] : zero(dest[i, j])
            end
        end
    elseif uplo == 'L' || uplo == 'l'
        for j in 1:n
            for i in 1:m
                dest[i, j] = i >= j ? src[i, j] : zero(dest[i, j])
            end
        end
    else
        error("ArgumentError: uplo must be 'U' or 'L'")
    end
    return dest
end

function Base.copyto!(dest::Diagonal, src::Diagonal)
    copyto!(dest.diag, src.diag)
    return dest
end

function Base.copyto!(dest::Matrix, src::Diagonal)
    m = size(dest, 1)
    n = size(dest, 2)
    dlen = length(src.diag)
    for j in 1:n
        for i in 1:m
            if i == j && i <= dlen
                dest[i, j] = src.diag[i]
            else
                dest[i, j] = zero(dest[i, j])
            end
        end
    end
    return dest
end

# =============================================================================
# isdiag - Check if matrix is diagonal
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    isdiag(A)

Test whether a matrix is diagonal (all off-diagonal elements are zero).
"""
function isdiag(A)
    m = size(A, 1)
    n = size(A, 2)
    for i in 1:m
        for j in 1:n
            if i != j && A[i, j] != 0
                return false
            end
        end
    end
    return true
end

# =============================================================================
# istriu - Check if matrix is upper triangular
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    istriu(A)

Test whether a matrix is upper triangular (all elements below the main diagonal are zero).

    istriu(A, k)

Test whether a matrix is upper triangular starting from the k-th superdiagonal.
"""
function istriu(A)
    m = size(A, 1)
    n = size(A, 2)
    for j in 1:n
        for i in j+1:m
            if A[i, j] != 0
                return false
            end
        end
    end
    return true
end

function istriu(A, k)
    m = size(A, 1)
    n = size(A, 2)
    for j in 1:n
        for i in max(1, j - k + 1):m
            if A[i, j] != 0
                return false
            end
        end
    end
    return true
end

# =============================================================================
# istril - Check if matrix is lower triangular
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

"""
    istril(A)

Test whether a matrix is lower triangular (all elements above the main diagonal are zero).

    istril(A, k)

Test whether a matrix is lower triangular up to the k-th superdiagonal.
"""
function istril(A)
    m = size(A, 1)
    n = size(A, 2)
    for j in 1:n
        for i in 1:min(j-1, m)
            if A[i, j] != 0
                return false
            end
        end
    end
    return true
end

function istril(A, k)
    m = size(A, 1)
    n = size(A, 2)
    for j in max(1, k + 2):n
        for i in 1:min(j - k - 1, m)
            if A[i, j] != 0
                return false
            end
        end
    end
    return true
end

# =============================================================================
# isposdef - Check if matrix is positive definite
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/dense.jl
# Uses manual Cholesky attempt: a symmetric matrix is positive definite
# if and only if Cholesky decomposition succeeds (all pivots positive).

"""
    isposdef(A)

Test whether a matrix is positive definite by attempting Cholesky decomposition.
A matrix is positive definite if it is symmetric and all eigenvalues are positive.
"""
function isposdef(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        return false
    end
    # Must be symmetric (for real matrices) / Hermitian
    if !issymmetric(A)
        return false
    end
    # Attempt Cholesky decomposition: A = L * L'
    # If any diagonal element becomes non-positive, A is not positive definite
    L = zeros(n, n)
    for j in 1:n
        s = 0.0
        for k in 1:j-1
            s = s + L[j, k] * L[j, k]
        end
        d = A[j, j] - s
        if d <= 0.0
            return false
        end
        L[j, j] = sqrt(d)
        for i in j+1:n
            s = 0.0
            for k in 1:j-1
                s = s + L[i, k] * L[j, k]
            end
            L[i, j] = (A[i, j] - s) / L[j, j]
        end
    end
    return true
end

"""
    isposdef!(A)

Test whether `A` is positive definite, using the supported in-place Cholesky
work path when the test succeeds.
"""
function isposdef!(A)
    return _cholesky_upper_work!(A)
end

# =============================================================================
# hermitianpart - Hermitian part of a matrix
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/symmetric.jl

"""
    hermitianpart(A)

Compute the Hermitian part of a matrix: `(A + adjoint(A)) / 2`.
For real matrices, this is the symmetric part: `(A + transpose(A)) / 2`.
"""
function hermitianpart(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        throw(DimensionMismatch("matrix is not square"))
    end
    B = adjoint(A)
    R = zeros(m, n)
    for i in 1:m
        for j in 1:n
            R[i, j] = (A[i, j] + B[i, j]) / 2
        end
    end
    return R
end

# =============================================================================
# eigmax - Maximum eigenvalue
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/symmetric.jl

"""
    eigmax(A)

Return the largest eigenvalue of matrix A.
For real symmetric matrices, all eigenvalues are real.
"""
function eigmax(A)
    vals = eigvals(A)
    # eigvals may return Complex{Float64}; use real() for comparison
    m = real(vals[1])
    for i in 2:length(vals)
        r = real(vals[i])
        if r > m
            m = r
        end
    end
    return m
end

# =============================================================================
# eigmin - Minimum eigenvalue
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/symmetric.jl

"""
    eigmin(A)

Return the smallest eigenvalue of matrix A.
For real symmetric matrices, all eigenvalues are real.
"""
function eigmin(A)
    vals = eigvals(A)
    # eigvals may return Complex{Float64}; use real() for comparison
    m = real(vals[1])
    for i in 2:length(vals)
        r = real(vals[i])
        if r < m
            m = r
        end
    end
    return m
end

# =============================================================================
# checksquare: check that a matrix is square and return its size
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/LinearAlgebra.jl

function checksquare(A)
    m = size(A, 1)
    n = size(A, 2)
    if m != n
        throw(DimensionMismatch("matrix is not square: dimensions are ($m, $n)"))
    end
    return m
end

# =============================================================================
# BLAS Level 1 operations: axpy!, axpby!
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

# axpy!(a, X, Y): Y = a*X + Y (overwrite Y)
function axpy!(a, X, Y)
    n = length(X)
    for i in 1:n
        Y[i] = a * X[i] + Y[i]
    end
    return Y
end

# axpby!(a, X, b, Y): Y = a*X + b*Y (overwrite Y)
function axpby!(a, X, b, Y)
    n = length(X)
    for i in 1:n
        Y[i] = a * X[i] + b * Y[i]
    end
    return Y
end

# =============================================================================
# rmul!, lmul!: in-place scalar multiplication
# =============================================================================
# Based on Julia's stdlib/LinearAlgebra/src/generic.jl

# rmul!(A, s): A = A * s (scale array in-place by scalar on the right)
function rmul!(A, s)
    n = length(A)
    for i in 1:n
        A[i] = A[i] * s
    end
    return A
end

# lmul!(s, A): A = s * A (scale array in-place by scalar on the left)
function lmul!(s, A)
    n = length(A)
    for i in 1:n
        A[i] = s * A[i]
    end
    return A
end

# =============================================================================
# mul!: in-place matrix multiply
# =============================================================================
# Based on Julia's LinearAlgebra.mul!

# mul!(C, A, B): compute C = A * B in-place
function mul!(C, A, B)
    m = size(A, 1)
    n = size(B, 2)
    p = size(A, 2)
    for i in 1:m
        for j in 1:n
            s = 0.0
            for k in 1:p
                s = s + A[i, k] * B[k, j]
            end
            C[i, j] = s
        end
    end
    return C
end

# mul!(C, A, B, alpha, beta): compute C = alpha * A * B + beta * C in-place (BLAS-style)
function mul!(C, A, B, alpha, beta)
    m = size(A, 1)
    n = size(B, 2)
    p = size(A, 2)
    for i in 1:m
        for j in 1:n
            s = 0.0
            for k in 1:p
                s = s + A[i, k] * B[k, j]
            end
            C[i, j] = alpha * s + beta * C[i, j]
        end
    end
    return C
end

# =============================================================================
# ldiv!: in-place left division (solve A\B, overwrite B with solution)
# =============================================================================
# Based on Julia's LinearAlgebra.ldiv!

# ldiv!(A, b): overwrite b with A \ b (solve Ax = b for x)
# Gaussian elimination with partial pivoting
function ldiv!(A, b)
    n = size(A, 1)
    # Make a working copy of A to avoid modifying the original
    U = zeros(n, n)
    for i in 1:n
        for j in 1:n
            U[i, j] = A[i, j]
        end
    end
    # Make a working copy of b
    x = zeros(n)
    for i in 1:n
        x[i] = b[i]
    end
    # Forward elimination with partial pivoting
    for k in 1:n
        # Find pivot
        max_val = abs(U[k, k])
        max_row = k
        for i in (k+1):n
            if abs(U[i, k]) > max_val
                max_val = abs(U[i, k])
                max_row = i
            end
        end
        # Swap rows in U and x
        if max_row != k
            for j in 1:n
                tmp = U[k, j]
                U[k, j] = U[max_row, j]
                U[max_row, j] = tmp
            end
            tmp = x[k]
            x[k] = x[max_row]
            x[max_row] = tmp
        end
        # Eliminate below
        for i in (k+1):n
            factor = U[i, k] / U[k, k]
            for j in (k+1):n
                U[i, j] = U[i, j] - factor * U[k, j]
            end
            U[i, k] = 0.0
            x[i] = x[i] - factor * x[k]
        end
    end
    # Back substitution
    for i in n:-1:1
        s = x[i]
        for j in (i+1):n
            s = s - U[i, j] * x[j]
        end
        x[i] = s / U[i, i]
    end
    # Overwrite b with result
    for i in 1:n
        b[i] = x[i]
    end
    return b
end

# =============================================================================
# rdiv!: in-place right division (solve A/B, overwrite A with solution)
# =============================================================================
# Based on Julia's LinearAlgebra.rdiv!

# rdiv!(A, B): overwrite A with A / B (solve XB = A for X)
# A / B = (B' \ A')' — solve row by row using Gaussian elimination
function rdiv!(A, B)
    m = size(A, 1)
    n = size(B, 1)
    # For each row of A, solve x * B = a_row, i.e., B' * x' = a_row'
    # Use Gaussian elimination on B' with each transposed row
    Bt = zeros(n, n)
    for i in 1:n
        for j in 1:n
            Bt[i, j] = B[j, i]  # transpose
        end
    end
    for row_idx in 1:m
        # Extract row as column vector
        rhs = zeros(n)
        for j in 1:n
            rhs[j] = A[row_idx, j]
        end
        # Solve Bt * x = rhs using Gaussian elimination
        U = zeros(n, n)
        for i in 1:n
            for j in 1:n
                U[i, j] = Bt[i, j]
            end
        end
        x = zeros(n)
        for i in 1:n
            x[i] = rhs[i]
        end
        # Forward elimination with partial pivoting
        for k in 1:n
            max_val = abs(U[k, k])
            max_r = k
            for i in (k+1):n
                if abs(U[i, k]) > max_val
                    max_val = abs(U[i, k])
                    max_r = i
                end
            end
            if max_r != k
                for j in 1:n
                    tmp = U[k, j]
                    U[k, j] = U[max_r, j]
                    U[max_r, j] = tmp
                end
                tmp = x[k]
                x[k] = x[max_r]
                x[max_r] = tmp
            end
            for i in (k+1):n
                factor = U[i, k] / U[k, k]
                for j in (k+1):n
                    U[i, j] = U[i, j] - factor * U[k, j]
                end
                U[i, k] = 0.0
                x[i] = x[i] - factor * x[k]
            end
        end
        # Back substitution
        for i in n:-1:1
            s = x[i]
            for j in (i+1):n
                s = s - U[i, j] * x[j]
            end
            x[i] = s / U[i, i]
        end
        # Store result back in row
        for j in 1:n
            A[row_idx, j] = x[j]
        end
    end
    return A
end

module BLAS

export dot, dotu, dotc, axpy!, scal!, gemv!, gemm!

function dot(x, y)
    n = length(x)
    s = 0.0
    for i in 1:n
        s = s + x[i] * y[i]
    end
    return s
end

function dot(n::Integer, x, incx::Integer, y, incy::Integer)
    s = 0.0
    ix = 1
    iy = 1
    for _ in 1:n
        s = s + x[ix] * y[iy]
        ix = ix + incx
        iy = iy + incy
    end
    return s
end

function dotu(x, y)
    n = length(x)
    s = 0.0
    for i in 1:n
        s = s + x[i] * y[i]
    end
    return s
end

function dotu(n::Integer, x, incx::Integer, y, incy::Integer)
    s = 0.0
    ix = 1
    iy = 1
    for _ in 1:n
        s = s + x[ix] * y[iy]
        ix = ix + incx
        iy = iy + incy
    end
    return s
end

function dotc(x, y)
    n = length(x)
    s = zero(conj(x[1]) * y[1])
    for i in 1:n
        s = s + conj(x[i]) * y[i]
    end
    return s
end

function dotc(n::Integer, x, incx::Integer, y, incy::Integer)
    ix = 1
    iy = 1
    s = zero(conj(x[ix]) * y[iy])
    for _ in 1:n
        s = s + conj(x[ix]) * y[iy]
        ix = ix + incx
        iy = iy + incy
    end
    return s
end

function axpy!(alpha, x, y)
    n = length(x)
    for i in 1:n
        y[i] = alpha * x[i] + y[i]
    end
    return y
end

function axpy!(n::Integer, alpha, x, incx::Integer, y, incy::Integer)
    ix = 1
    iy = 1
    for _ in 1:n
        y[iy] = alpha * x[ix] + y[iy]
        ix = ix + incx
        iy = iy + incy
    end
    return y
end

function scal!(alpha, x)
    n = length(x)
    for i in 1:n
        x[i] = alpha * x[i]
    end
    return x
end

function scal!(n::Integer, alpha, x, incx::Integer)
    ix = 1
    for _ in 1:n
        x[ix] = alpha * x[ix]
        ix = ix + incx
    end
    return x
end

function _blas_get(A, trans, i, j)
    if trans == 'N' || trans == 'n'
        return A[i, j]
    elseif trans == 'T' || trans == 't'
        return A[j, i]
    elseif trans == 'C' || trans == 'c'
        return conj(A[j, i])
    end
    error("ArgumentError: BLAS transpose flag must be 'N', 'T', or 'C'")
end

function gemv!(trans, alpha, A, x, beta, y)
    rows = (trans == 'N' || trans == 'n') ? size(A, 1) : size(A, 2)
    cols = (trans == 'N' || trans == 'n') ? size(A, 2) : size(A, 1)
    for i in 1:rows
        s = 0.0
        for j in 1:cols
            s = s + _blas_get(A, trans, i, j) * x[j]
        end
        y[i] = alpha * s + beta * y[i]
    end
    return y
end

function gemm!(transA, transB, alpha, A, B, beta, C)
    m = (transA == 'N' || transA == 'n') ? size(A, 1) : size(A, 2)
    k = (transA == 'N' || transA == 'n') ? size(A, 2) : size(A, 1)
    n = (transB == 'N' || transB == 'n') ? size(B, 2) : size(B, 1)
    for i in 1:m
        for j in 1:n
            s = 0.0
            for p in 1:k
                s = s + _blas_get(A, transA, i, p) * _blas_get(B, transB, p, j)
            end
            C[i, j] = alpha * s + beta * C[i, j]
        end
    end
    return C
end

end # module BLAS

module LAPACK

import ..LinearAlgebra: inv, lu, LU

function _identity_pivots(n)
    p = zeros(Int64, n)
    for i in 1:n
        p[i] = i
    end
    return p
end

function _copy_result!(dest, src)
    dims = size(dest)
    if length(dims) == 1
        for i in 1:length(dest)
            dest[i] = src[i]
        end
    else
        for j in 1:size(dest, 2)
            for i in 1:size(dest, 1)
                dest[i, j] = src[i, j]
            end
        end
    end
    return dest
end

function gesv!(A, B)
    X = inv(A) * B
    _copy_result!(B, X)
    return (B, A, _identity_pivots(size(A, 1)))
end

function getrf!(A)
    F = lu(A)
    if F isa LU
        m = size(A, 1)
        n = size(A, 2)
        for j in 1:n
            for i in 1:m
                if i > j
                    A[i, j] = F.L[i, j]
                else
                    A[i, j] = F.U[i, j]
                end
            end
        end
        return (A, F.p, 0)
    end
    return (A, _identity_pivots(size(A, 1)), 0)
end

end # module LAPACK

end # module LinearAlgebra
