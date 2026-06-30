# Multivariate optimize entry points. Adapted from upstream
# `src/multivariate/optimize/interface.jl`.

# Default derivative-free method is NelderMead.
optimize(f, x0::AbstractArray) = optimize(f, x0, NelderMead(), Options())
optimize(f, x0::AbstractArray, options::Options) = optimize(f, x0, NelderMead(), options)

# First-order optimization with a user-supplied in-place gradient g!(G, x).
function optimize(f, g!, x0::AbstractArray, method::GradientDescent; kwargs...)
    return optimize(f, g!, x0, method, Options(; kwargs...))
end

function optimize(f, g!, x0::AbstractArray, method::GradientDescent, options::Options)
    d = OnceDifferentiable(f, g!, x0)
    return _gradient_descent(d, x0, method, options)
end

# ── BFGS ─────────────────────────────────────────────────────────────────────
# No-gradient form: central finite-difference gradient (autodiff = :finite).
function optimize(f, x0::AbstractArray, method::BFGS; kwargs...)
    return optimize(f, x0, method, Options(; kwargs...))
end

function optimize(f, x0::AbstractArray, method::BFGS, options::Options)
    d = OnceDifferentiable(f, x0)
    return _bfgs(d, x0, method, options)
end

# User-gradient form: in-place g!(G, x).
function optimize(f, g!, x0::AbstractArray, method::BFGS; kwargs...)
    return optimize(f, g!, x0, method, Options(; kwargs...))
end

function optimize(f, g!, x0::AbstractArray, method::BFGS, options::Options)
    d = OnceDifferentiable(f, g!, x0)
    return _bfgs(d, x0, method, options)
end
