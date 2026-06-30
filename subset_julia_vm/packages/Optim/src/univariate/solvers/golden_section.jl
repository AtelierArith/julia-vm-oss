# Golden-section search. Adapted from upstream
# `src/univariate/solvers/golden_section.jl` (trace machinery removed).

"""
    GoldenSection()

Golden-section search for minimizing a univariate function on `[a, b]`.
"""
struct GoldenSection <: UnivariateOptimizer end

summary(io::IO, ::GoldenSection) = print(io, "Golden Section Search")

function optimize(
    f,
    x_lower::Real,
    x_upper::Real,
    mo::GoldenSection;
    rel_tol::Real = sqrt(eps(Float64)),
    abs_tol::Real = eps(Float64),
    iterations::Integer = 1_000,
    store_trace::Bool = false,
    show_trace::Bool = false,
    show_warnings::Bool = true,
    callback = nothing,
    show_every::Integer = 1,
    extended_trace::Bool = false,
    time_limit = Inf,
)
    x_lower = Float64(x_lower)
    x_upper = Float64(x_upper)
    if x_lower > x_upper
        error("x_lower must be less than x_upper")
    end
    rtol = Float64(rel_tol)
    atol = Float64(abs_tol)

    initial_lower = x_lower
    initial_upper = x_upper

    golden_ratio = 0.5 * (3.0 - sqrt(5.0))

    new_minimizer = x_lower + golden_ratio * (x_upper - x_lower)
    new_minimum = f(new_minimizer)
    fcalls = 1

    iteration = 0
    is_converged = false

    while iteration < iterations
        x_tol = rtol * abs(new_minimizer) + atol
        x_midpoint = (x_upper + x_lower) / 2

        if abs(new_minimizer - x_midpoint) <= 2 * x_tol - (x_upper - x_lower) / 2
            is_converged = true
            break
        end

        iteration += 1

        if x_upper - new_minimizer > new_minimizer - x_lower
            new_x = new_minimizer + golden_ratio * (x_upper - new_minimizer)
            new_f = f(new_x)
            fcalls += 1
            if new_f < new_minimum
                x_lower = new_minimizer
                new_minimizer = new_x
                new_minimum = new_f
            else
                x_upper = new_x
            end
        else
            new_x = new_minimizer - golden_ratio * (new_minimizer - x_lower)
            new_f = f(new_x)
            fcalls += 1
            if new_f < new_minimum
                x_upper = new_minimizer
                new_minimizer = new_x
                new_minimum = new_f
            else
                x_lower = new_x
            end
        end
    end

    return UnivariateOptimizationResults(
        mo,
        initial_lower,
        initial_upper,
        new_minimizer,
        new_minimum,
        iteration,
        rtol,
        atol,
        fcalls,
        is_converged,
    )
end
