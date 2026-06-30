# Multivariate normal distribution (Issue #7178, Phase 4).
#
# MvNormal(μ, Σ): μ is the mean vector, Σ a symmetric positive-definite
# covariance matrix.
#
# This file deliberately avoids `using LinearAlgebra`: importing LinearAlgebra
# inside a user/package module currently fails to load (Issue #7245). Instead it
# implements the small dense linear algebra it needs — a lower Cholesky factor
# and forward substitution — in pure Julia. Matrix·vector `*` is a builtin
# operator and works without LinearAlgebra, so sampling uses it directly.
#
# Open question #1 (store a `Cholesky` factor object vs a raw matrix) is resolved
# by storing the raw lower factor `L` (Σ = L·Lᵀ) plus `logdet(Σ)`; the VM has no
# `Cholesky` wrapper type. An inner constructor (`new`) precomputes them.

# Lower Cholesky factor L (Σ = L·Lᵀ) via the Cholesky–Banachiewicz algorithm.
function _chol_lower(S)
    n = size(S, 1)
    L = zeros(n, n)
    for i in 1:n
        for j in 1:i
            s = 0.0
            for k in 1:(j - 1)
                s += L[i, k] * L[j, k]
            end
            if i == j
                d = S[i, i] - s
                if d <= 0.0
                    throw(ArgumentError("MvNormal: Σ is not positive definite."))
                end
                L[i, j] = sqrt(d)
            else
                L[i, j] = (S[i, j] - s) / L[j, j]
            end
        end
    end
    return L
end

# Forward substitution: solve L·y = b for a lower-triangular L.
function _forward_solve(L, b)
    n = length(b)
    y = zeros(n)
    for i in 1:n
        s = b[i]
        for k in 1:(i - 1)
            s -= L[i, k] * y[k]
        end
        y[i] = s / L[i, i]
    end
    return y
end

# Fields are left untyped, and the moments are assembled by an *outer*
# constructor of different arity (2-arg → the 4-arg default), because the VM
# does not register an inner constructor across module boundaries and does not
# match Any-typed computed values against typed default-constructor fields
# (Issues #7235 / #7240). `L` is the lower Cholesky factor (Σ = L·Lᵀ) and
# `logdetΣ = log(det Σ)`.
struct MvNormal <: Distribution{Multivariate, Continuous}
    μ
    Σ
    L
    logdetΣ
end

function MvNormal(μ, Σ)
    n = length(μ)
    if size(Σ, 1) != n || size(Σ, 2) != n
        throw(ArgumentError("MvNormal: Σ must be a square matrix matching length(μ)."))
    end
    L = _chol_lower(Σ)
    ld = 0.0
    for i in 1:n
        ld += log(L[i, i])
    end
    return MvNormal(collect(μ), Σ, L, 2.0 * ld)
end

# Zero-mean convenience constructor (single matrix argument).
MvNormal(Σ) = MvNormal(zeros(size(Σ, 1)), Σ)

params(d::MvNormal) = (d.μ, d.Σ)
mean(d::MvNormal) = d.μ
cov(d::MvNormal) = d.Σ
function var(d::MvNormal)
    n = length(d.μ)
    v = zeros(n)
    for i in 1:n
        v[i] = d.Σ[i, i]
    end
    return v
end
# Dimension of the distribution (number of variates).
dim(d::MvNormal) = length(d.μ)

function logpdf(d::MvNormal, x::AbstractVector)
    k = length(d.μ)
    diff = x - d.μ
    # quad = diffᵀ Σ⁻¹ diff = ‖L⁻¹ diff‖² with y = L⁻¹ diff (forward solve).
    y = _forward_solve(d.L, diff)
    quad = 0.0
    for i in 1:k
        quad += y[i]^2
    end
    return -0.5 * (k * log(2.0 * pi) + d.logdetΣ + quad)
end
pdf(d::MvNormal, x::AbstractVector) = exp(logpdf(d, x))

insupport(d::MvNormal, x::AbstractVector) = length(x) == length(d.μ)

# Sampling: μ + L·z with z ~ N(0, I_k).
function _rand_scalar(rng, d::MvNormal)
    z = zeros(length(d.μ))
    for i in 1:length(z)
        z[i] = randn(rng)
    end
    return d.μ + d.L * z
end
rand(d::MvNormal) = _rand_scalar(Random.default_rng(), d)
rand(rng, d::MvNormal) = _rand_scalar(rng, d)
