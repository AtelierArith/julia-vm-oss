# Linear algebra for the Symbolics subset (Issue #7889, Epic #7888).
#
# Symbolic-element arrays (`AbstractMatrix{<:Num}`) cannot use the VM's numeric
# `matmul` builtin: it assumes Float64/Complex storage and rejects `Num`
# (`matmul: expected Complex struct, got Symbolics.Num`). Upstream Julia's `*`
# is fully generic over the element type; the subset mirrors that here with
# element-type-generic loops that accumulate symbolic `Num` values and store
# them in a `similar`-allocated result whose eltype is inferred from the first
# product (the subset's stand-in for upstream's `promote_op`/`matprod` result
# eltype).
#
# These methods constrain the *left* operand to `AbstractMatrix{<:Num}`, so they
# are strictly more specific than the generic numeric path and win dispatch for
# symbolic matrices. Purely numeric arrays never reach this dispatch — the
# compiler routes `Matrix{Float64} * …` straight to the fast `Instr::MatMul`
# builtin — so there is no performance regression for numeric linear algebra.
# The right operand is left unconstrained on purpose: `[x, y]` builds a
# `Vector{Any}` (not `Vector{<:Num}`) in the subset, and mixed products such as
# `A * [1, 2]` must also be absorbed by the symbolic `+`/`*` methods.

# A (m×n) * b (n-vector) -> (m-vector).
function Base.:*(A::AbstractMatrix{<:Num}, b::AbstractVector)::Any
    m = size(A, 1)
    n = size(A, 2)
    if n != length(b)
        error("DimensionMismatch: matrix A has $(n) columns, but vector b has $(length(b)) elements")
    end
    # The first product fixes the result element type (e.g. `Num`); `similar`
    # then allocates a matching result vector that can store symbolic values.
    s = A[1, 1] * b[1]
    for p in 2:n
        s = s + A[1, p] * b[p]
    end
    y = similar(b, typeof(s), m)
    y[1] = s
    for i in 2:m
        acc = A[i, 1] * b[1]
        for p in 2:n
            acc = acc + A[i, p] * b[p]
        end
        y[i] = acc
    end
    return y
end

# A (m×k) * B (k×n) -> (m×n).
function Base.:*(A::AbstractMatrix{<:Num}, B::AbstractMatrix)::Any
    m = size(A, 1)
    k = size(A, 2)
    if k != size(B, 1)
        error("DimensionMismatch: matrix A has $(k) columns, but matrix B has $(size(B, 1)) rows")
    end
    n = size(B, 2)
    s = A[1, 1] * B[1, 1]
    for p in 2:k
        s = s + A[1, p] * B[p, 1]
    end
    C = similar(A, typeof(s), m, n)
    for i in 1:m
        for j in 1:n
            acc = A[i, 1] * B[1, j]
            for p in 2:k
                acc = acc + A[i, p] * B[p, j]
            end
            C[i, j] = acc
        end
    end
    return C
end

# ── Determinant, inverse and linear solve (Issue #7892) ──────────────────────
# `det` / `inv` / `\` over symbolic matrices via Laplace (cofactor) expansion,
# so they never reach the VM's numeric `det`/`inv` builtins (which assume
# Float64/Complex elements). `\` needs no method here: the LinearAlgebra stdlib
# already defines `\(A::AbstractMatrix, b) = inv(A) * b`, which routes through the
# symbolic inverse below and the symbolic matmul above.

import LinearAlgebra: det, inv

# The (di, dj) minor, built element-by-element into a fresh `similar` matrix.
# Built by copying rather than `A[rows, cols]` slicing on purpose: the slice path
# can mis-specialize and report "expected I64, got StructRef" for symbolic
# elements, and a filtered comprehension can silently drop rows under
# specialization (Issue #7891), so the explicit copy is the robust construction.
function _sym_minor(A, di, dj)::Any
    n = size(A, 1)
    m = size(A, 2)
    M = similar(A, n - 1, m - 1)
    ri = 0
    for i in 1:n
        if i != di
            ri = ri + 1
            cj = 0
            for j in 1:m
                if j != dj
                    cj = cj + 1
                    M[ri, cj] = A[i, j]
                end
            end
        end
    end
    return M
end

# `det` via Laplace expansion along the first row. The LinearAlgebra stdlib's
# generic `det(A)` is untyped, so this parametric method wins runtime dispatch
# for symbolic matrices. `simplify` collapses cancelling cofactor terms, so e.g.
# `det([x y; x y])` is structurally `0`.
function det(A::AbstractMatrix{<:Num})::Num
    n = size(A, 1)
    n == 1 && return A[1, 1]
    if n == 2
        return simplify(A[1, 1] * A[2, 2] - A[1, 2] * A[2, 1])
    end
    s = A[1, 1] * det(_sym_minor(A, 1, 1))
    for j in 2:n
        s = s + (-1)^(1 + j) * A[1, j] * det(_sym_minor(A, 1, j))
    end
    return simplify(s)
end

# `inv` via the adjugate (transpose of the cofactor matrix) divided by `det`.
function _sym_inv(A)::Any
    n = size(A, 1)
    idet = 1 / det(A)
    B = similar(A)
    if n == 1
        B[1, 1] = idet
        return B
    end
    for i in 1:n
        for j in 1:n
            # adjugate transposes the cofactor index pair (j, i)
            B[i, j] = simplify((-1)^(i + j) * det(_sym_minor(A, j, i)) * idet)
        end
    end
    return B
end

# `inv(A::AbstractMatrix{<:Num})` extends the LinearAlgebra `inv` and is
# more specific than the numeric `inv(::AbstractMatrix)` builtin-forwarder, so it
# wins runtime dispatch for symbolic matrices (the parametric-vs-bare specificity
# was fixed in Issue #8025; numeric matrices keep using the builtin).
function inv(A::AbstractMatrix{<:Num})::Any
    return _sym_inv(A)
end
