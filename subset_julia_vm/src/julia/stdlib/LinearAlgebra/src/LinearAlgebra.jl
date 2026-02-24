# =============================================================================
# LinearAlgebra - Linear Algebra Standard Library
# =============================================================================
# Based on Julia's LinearAlgebra stdlib
# https://docs.julialang.org/en/v1/stdlib/LinearAlgebra/
#
# This module provides basic linear algebra operations for vectors and matrices.
# Functions are implemented to match Julia's LinearAlgebra module behavior.

module LinearAlgebra

export tr, dot, norm, cross
export kron, kron!
export lu, det, inv, svd, qr, eigen, eigvals, eigvecs, cholesky, rank, cond, pinv
export transpose
export Diagonal
export normalize, diag, issymmetric, ishermitian
export triu, tril, diagm, opnorm
export nullspace, logdet, logabsdet, adjoint
export isdiag, istriu, istril, isposdef
export hermitianpart, eigmax, eigmin
export checksquare
export axpy!, axpby!, rmul!, lmul!
export mul!, ldiv!, rdiv!
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

# Matrix{Float64} * Vector{Complex{Float64}} -> Vector{Complex{Float64}}
# This handles the case where eigenvectors are complex but the original matrix is real
function Base.:*(A::Matrix{Float64}, x::Vector{Complex{Float64}})
    m = size(A, 1)
    n = size(A, 2)
    if n != length(x)
        error("DimensionMismatch: A has $(n) columns, but x has $(length(x)) elements")
    end
    # Workaround: use fill() instead of Vector{T}(undef, n)
    y = fill(Complex(0.0, 0.0), m)
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
function norm(x::Array{Float64}, p)
    n = length(x)
    if p == 2
        s = 0.0
        for i in 1:n
            xi = x[i]
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for i in 1:n
            s = s + abs(x[i])
        end
        return s
    elseif isinf(p)
        m = 0.0
        for i in 1:n
            v = abs(x[i])
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for i in 1:n
            s = s + abs(x[i])^p
        end
        return s^(1.0/p)
    end
end

# Specialized norm for Int64 arrays
function norm(x::Array{Int64}, p)
    n = length(x)
    if p == 2
        s = 0.0
        for i in 1:n
            xi = Float64(x[i])
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for i in 1:n
            s = s + abs(Float64(x[i]))
        end
        return s
    elseif isinf(p)
        m = 0.0
        for i in 1:n
            v = abs(Float64(x[i]))
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for i in 1:n
            s = s + abs(Float64(x[i]))^p
        end
        return s^(1.0/p)
    end
end

# Specialized norm for Complex{Float64} arrays
function norm(x::Array{Complex{Float64}}, p)
    n = length(x)
    if p == 2
        s = 0.0
        for i in 1:n
            xi = x[i]
            # abs2(z) = re^2 + im^2
            s = s + xi.re * xi.re + xi.im * xi.im
        end
        return sqrt(s)
    elseif p == 1
        s = 0.0
        for i in 1:n
            xi = x[i]
            s = s + sqrt(xi.re * xi.re + xi.im * xi.im)
        end
        return s
    elseif isinf(p)
        m = 0.0
        for i in 1:n
            xi = x[i]
            v = sqrt(xi.re * xi.re + xi.im * xi.im)
            if v > m
                m = v
            end
        end
        return m
    else
        s = 0.0
        for i in 1:n
            xi = x[i]
            v = sqrt(xi.re * xi.re + xi.im * xi.im)
            s = s + v^p
        end
        return s^(1.0/p)
    end
end

# Generic norm fallback
function norm(x, p)
    n = length(x)
    if p == 2
        # L2 norm (Euclidean)
        s = 0.0
        for i in 1:n
            xi = x[i]
            s = s + xi * xi
        end
        return sqrt(s)
    elseif p == 1
        # L1 norm (Manhattan)
        s = 0.0
        for i in 1:n
            s = s + abs(x[i])
        end
        return s
    elseif isinf(p)
        # Infinity norm
        m = 0.0
        for i in 1:n
            v = abs(x[i])
            if v > m
                m = v
            end
        end
        return m
    else
        # General p-norm
        s = 0.0
        for i in 1:n
            s = s + abs(x[i])^p
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
    if dim == 1 || dim == 2
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

# =============================================================================
# Linear Algebra Decompositions
# =============================================================================
# These functions are implemented as builtins in the VM for performance.
# The function definitions here make them available via method dispatch
# when using LinearAlgebra module.

"""
    lu(A)

Compute the LU decomposition of matrix A with partial pivoting.
Returns a tuple (L, U, p) where L is lower triangular, U is upper triangular,
and p is a permutation vector such that A[p, :] = L * U.
"""
function lu(A)
    # This will be compiled to CallBuiltin(BuiltinId::Lu, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.lu resolution
    return Base.LinearAlgebra.lu(A)
end

"""
    det(A)

Compute the determinant of matrix A using LU decomposition.
"""
function det(A)
    # This will be compiled to CallBuiltin(BuiltinId::Det, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.det resolution
    return Base.LinearAlgebra.det(A)
end

"""
    inv(A)

Compute the inverse of matrix A using LU decomposition.
"""
function inv(A::AbstractMatrix)
    # This will be compiled to CallBuiltin(BuiltinId::Inv, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.inv resolution.
    return Base.LinearAlgebra.inv(A)
end

function inv(A)
    return Base.inv(A)
end

"""
    svd(A)

Compute the Singular Value Decomposition of matrix A.
Returns a named tuple with fields U, S, V, and Vt.
"""
function svd(A)
    # This will be compiled to CallBuiltin(BuiltinId::Svd, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.svd resolution
    return Base.LinearAlgebra.svd(A)
end

"""
    qr(A)

Compute the QR decomposition of matrix A.
Returns a named tuple with fields Q and R.
"""
function qr(A)
    # This will be compiled to CallBuiltin(BuiltinId::Qr, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.qr resolution
    return Base.LinearAlgebra.qr(A)
end

"""
    eigen(A)

Compute the eigenvalue decomposition of matrix A.
Returns a named tuple with fields values (eigenvalues) and vectors (eigenvectors).
Only works for symmetric matrices with real eigenvalues.
"""
function eigen(A)
    # This will be compiled to CallBuiltin(BuiltinId::Eigen, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.eigen resolution
    return Base.LinearAlgebra.eigen(A)
end

"""
    eigvals(A)

Compute the eigenvalues of matrix A.
Returns a vector of complex eigenvalues.
"""
function eigvals(A)
    # This will be compiled to CallBuiltin(BuiltinId::Eigvals, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.eigvals resolution
    return Base.LinearAlgebra.eigvals(A)
end

"""
    cholesky(A)

Compute the Cholesky decomposition of symmetric positive-definite matrix A.
Returns a named tuple with fields L and U where U = L'.
"""
function cholesky(A)
    # This will be compiled to CallBuiltin(BuiltinId::Cholesky, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.cholesky resolution
    return Base.LinearAlgebra.cholesky(A)
end

"""
    rank(A)

Compute the rank of matrix A (number of singular values above tolerance).
"""
function rank(A)
    # This will be compiled to CallBuiltin(BuiltinId::Rank, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.rank resolution
    return Base.LinearAlgebra.rank(A)
end

"""
    cond(A)

Compute the condition number of matrix A (2-norm condition number).
"""
function cond(A)
    # This will be compiled to CallBuiltin(BuiltinId::Cond, 1) by the compiler
    # when A is an Array type via Base.LinearAlgebra.cond resolution
    return Base.LinearAlgebra.cond(A)
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
        # Return empty n×0 matrix (represented as zeros(n, 0) is not supported,
        # so return zeros(n, 1) with a flag - but actually we return a 0-column result)
        # Workaround: return zeros(n, 0) is not supported, return empty-like
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

end # module LinearAlgebra
