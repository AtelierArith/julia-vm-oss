# Small numeric helpers used by the multivariate solvers.
# Kept self-contained (no LinearAlgebra/Statistics dispatch) so the MVP solver
# loops are deterministic and robust on the no-JIT runtime.

# Maximum absolute value of a vector (∞-norm of the gradient residual).
function _maxabs(v)
    m = 0.0
    for vi in v
        a = abs(vi)
        if a > m
            m = a
        end
    end
    return m
end

# Dot product of two equal-length vectors.
function _dot(a, b)
    s = 0.0
    for i in eachindex(a)
        s += a[i] * b[i]
    end
    return s
end

# Sample variance (corrected, divides by N-1), matching `Statistics.var`.
function _var(y)
    n = length(y)
    mu = 0.0
    for yi in y
        mu += yi
    end
    mu = mu / n
    s = 0.0
    for yi in y
        d = yi - mu
        s += d * d
    end
    return s / (n - 1)
end

# Ascending sort permutation (stable insertion sort; simplices are tiny).
function _sortperm(y)
    n = length(y)
    p = collect(1:n)
    for i in 2:n
        j = i
        while j > 1 && y[p[j-1]] > y[p[j]]
            tmp = p[j-1]
            p[j-1] = p[j]
            p[j] = tmp
            j -= 1
        end
    end
    return p
end

# Minimum value and its index.
function _findmin(y)
    best = y[1]
    idx = 1
    for i in 2:length(y)
        if y[i] < best
            best = y[i]
            idx = i
        end
    end
    return best, idx
end
