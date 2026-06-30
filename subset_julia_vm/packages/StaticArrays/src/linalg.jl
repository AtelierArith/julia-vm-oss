# Small static linear algebra (Issue #7461, Phase 5).
#
# transpose / adjoint / inv return a static matrix, so they go through
# `_smatrix_colmajor`, which selects a literal-size `SMatrix{M,N}` constructor
# (runtime-parameter `SMatrix{M,N}` construction is unsupported — Issue #8125).
# tr / diag / det return a scalar or `SVector` and so work for any small size via
# an index-based loop or a closed form. All paths index through `getindex`, so
# they are agnostic to the column-major backing layout (Issue #8084).
#
# dot / norm already have working implementations (VM dot fast path and the
# index-based `LinearAlgebra.norm` in arraymath.jl); they are exercised by the
# Phase 5 fixture but need no method here.

# Build an SMatrix from a length-`rows*cols`, column-major value list. The
# explicit (rows, cols) lets transpose flip a rectangular shape; the literal
# constructors sidestep the unsupported runtime-parameter `SMatrix{M,N}` path.
function _smatrix_colmajor(rows::Int64, cols::Int64, d)
    if rows == 1 && cols == 1
        return SMatrix{1,1}((d[1],))
    elseif rows == 2 && cols == 2
        return SMatrix{2,2}((d[1], d[2], d[3], d[4]))
    elseif rows == 3 && cols == 3
        return SMatrix{3,3}((d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9]))
    elseif rows == 4 && cols == 4
        return SMatrix{4,4}((d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8],
                             d[9], d[10], d[11], d[12], d[13], d[14], d[15], d[16]))
    elseif rows == 2 && cols == 3
        return SMatrix{2,3}((d[1], d[2], d[3], d[4], d[5], d[6]))
    elseif rows == 3 && cols == 2
        return SMatrix{3,2}((d[1], d[2], d[3], d[4], d[5], d[6]))
    end
    error("StaticArrays: matrix shape $(rows)×$(cols) is out of scope for this operation (Issue #7461)")
end

# B = transpose(A): B[i,j] = A[j,i]. Build B's column-major value list directly.
function LinearAlgebra.transpose(A::StaticMatrix)
    s = size(A)
    m = s[1]
    n = s[2]
    vals = []
    for bj in 1:m      # B's columns correspond to A's rows
        for bi in 1:n  # B's rows correspond to A's columns
            push!(vals, A[bj, bi])
        end
    end
    return _smatrix_colmajor(n, m, vals)
end

# NOTE: `adjoint(A::StaticMatrix)` (and the `A'` postfix) is intentionally NOT
# defined here. A bare `adjoint` call resolves to the generic `LinearAlgebra`
# `adjoint(A)` which is not overridden by a more-specific static method in sjulia
# (and defining one currently trips a compiler issue) — tracked in Issue #8132.
# For the real-valued Phase 5 MVP, use `transpose` (adjoint == transpose for reals).

# Trace: sum of the leading diagonal.
function LinearAlgebra.tr(A::StaticMatrix)
    s = size(A)
    n = s[1]
    acc = A[1, 1]
    for i in 2:n
        acc += A[i, i]
    end
    return acc
end

# Diagonal as a static vector.
function LinearAlgebra.diag(A::StaticMatrix)
    s = size(A)
    n = s[1] < s[2] ? s[1] : s[2]
    vals = []
    for i in 1:n
        push!(vals, A[i, i])
    end
    return SVector(vals...)
end

# Determinant via closed forms for 1×1 / 2×2 / 3×3 (larger sizes need an LU
# factorisation, deferred — Issue #7461).
function LinearAlgebra.det(A::StaticMatrix)
    s = size(A)
    n = s[1]
    if n == 1
        return A[1, 1]
    elseif n == 2
        return A[1, 1] * A[2, 2] - A[1, 2] * A[2, 1]
    elseif n == 3
        return A[1, 1] * (A[2, 2] * A[3, 3] - A[2, 3] * A[3, 2]) -
               A[1, 2] * (A[2, 1] * A[3, 3] - A[2, 3] * A[3, 1]) +
               A[1, 3] * (A[2, 1] * A[3, 2] - A[2, 2] * A[3, 1])
    end
    error("StaticArrays: det supports 1×1/2×2/3×3 only (Issue #7461)")
end

# Inverse via closed forms for 1×1 / 2×2 (upstream-stable; larger sizes deferred).
function LinearAlgebra.inv(A::StaticMatrix)
    s = size(A)
    n = s[1]
    if n == 1
        return SMatrix{1,1}((1 / A[1, 1],))
    elseif n == 2
        d = A[1, 1] * A[2, 2] - A[1, 2] * A[2, 1]
        # inv(A) = (1/d) [A22 -A12; -A21 A11]; column-major below.
        return SMatrix{2,2}((A[2, 2] / d, -A[2, 1] / d, -A[1, 2] / d, A[1, 1] / d))
    end
    error("StaticArrays: inv supports 1×1/2×2 only (Issue #7461)")
end
