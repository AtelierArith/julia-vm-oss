# Univariate optimize entry points. Adapted from upstream
# `src/univariate/optimize/interface.jl`.
#
# Integer / mixed-Real bounds are promoted to Float64 inside the solver methods,
# matching upstream's `optimize(f, lower, upper)` promotion behavior.

# Default method is Brent (matches upstream).
function optimize(f, lower::Real, upper::Real; method = Brent(), kwargs...)
    return optimize(f, lower, upper, method; kwargs...)
end
