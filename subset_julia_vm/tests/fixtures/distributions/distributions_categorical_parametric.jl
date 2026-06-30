# Issue #7263: a typed method (`var(d::Categorical)`, `mean`, `mode`, `quantile`,
# the module-local `ncategories`, …) extending an imported generic must beat the
# untyped `Statistics.var(arr)` / `Statistics.median(arr)` generic for the
# upstream-faithful *parametric* struct `Categorical{T<:Real}` with a typed
# `p::Vector{T}` field. Previously the parametric form lost `var` dispatch to the
# untyped generic (`MethodError: no method matching length(Categorical{Float64})`),
# which is why Categorical had been declared non-parametric as a workaround.
using Distributions

tol = 1e-9
ok = true

c = Categorical([0.2, 0.3, 0.5])

# The struct is the upstream-faithful parametric form.
ok = ok && (c isa Categorical)
ok = ok && (eltype(probs(c)) == Float64)

# Typed accessors / methods on the parametric struct, including the internal
# cross-method call `ncategories(d)` used inside `mean`/`var`/`mode`.
ok = ok && (ncategories(c) == 3)
ok = ok && (probs(c) == [0.2, 0.3, 0.5])
ok = ok && (abs(mean(c) - 2.3) < tol)
ok = ok && (abs(var(c) - 0.61) < tol)          # the previously-failing method
ok = ok && (abs(std(c) - 0.7810249675906654) < tol)
ok = ok && (mode(c) == 3)
ok = ok && (quantile(c, 0.5) == 2)
ok = ok && (median(c) == 2)
ok = ok && (minimum(c) == 1) && (maximum(c) == 3)
ok = ok && (support(c) == 1:3)

# Uniform constructor `Categorical(k)` over 1:k.
cu = Categorical(4)
ok = ok && (probs(cu) == [0.25, 0.25, 0.25, 0.25])
ok = ok && (ncategories(cu) == 4)
ok = ok && (abs(mean(cu) - 2.5) < tol)
ok = ok && (abs(var(cu) - 1.25) < tol)

ok
