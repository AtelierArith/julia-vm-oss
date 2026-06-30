using Distributions

tol = 1e-6
ok = true

# Normal MLE: μ̂ = x̄, σ̂ = sqrt(mean((x-μ̂)²))
dn = fit(Normal, [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0])
ok = ok && (abs(mean(dn) - 5.0) < tol)
ok = ok && (abs(std(dn) - 2.0) < tol)
# fit_mle alias
ok = ok && (abs(mean(fit_mle(Normal, [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0])) - 5.0) < tol)

# Bernoulli MLE: p̂ = x̄
db = fit(Bernoulli, [1, 0, 1, 1, 0])
ok = ok && (abs(succprob(db) - 0.6) < tol)

# Exponential MLE: θ̂ = x̄
de = fit(Exponential, [1.0, 2.0, 3.0, 4.0])
ok = ok && (abs(scale(de) - 2.5) < tol)

# Poisson MLE: λ̂ = x̄
dp = fit(Poisson, [1, 2, 3, 2, 2])
ok = ok && (abs(rate(dp) - 2.0) < tol)

# Geometric MLE: p̂ = 1 / (1 + x̄)
dg = fit(Geometric, [0, 1, 2, 0, 2])
ok = ok && (abs(succprob(dg) - 0.5) < tol)

# Uniform MLE: â = min(x), b̂ = max(x)
du = fit(Uniform, [1.0, 3.0, 2.0, 5.0, 4.0])
ok = ok && (minimum(du) == 1.0) && (maximum(du) == 5.0)

# MvNormal MLE: μ̂ = row means, Σ̂ = (1/n) Σ (xᵢ-μ̂)(xᵢ-μ̂)'
dm = fit(MvNormal, [1.0 2.0 3.0 4.0 5.0; 2.0 1.0 4.0 3.0 6.0])
ok = ok && (mean(dm) == [3.0, 3.2])
Σ = cov(dm)
ok = ok && (abs(Σ[1, 1] - 2.0) < tol)
ok = ok && (abs(Σ[1, 2] - 2.0) < tol)
ok = ok && (abs(Σ[2, 1] - 2.0) < tol)
ok = ok && (abs(Σ[2, 2] - 2.96) < tol)

ok
