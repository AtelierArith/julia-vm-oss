# Brent's method. Adapted from upstream `src/univariate/solvers/brent.jl`
# (trace machinery removed).

"""
    Brent()

Brent's method (parabolic interpolation + golden-section bisection) for
minimizing a univariate function on `[a, b]`.
"""
struct Brent <: UnivariateOptimizer end

summary(io::IO, ::Brent) = print(io, "Brent's Method")

function optimize(
    f,
    x_lower::Real,
    x_upper::Real,
    mo::Brent;
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
    step = 0.0
    old_step = 0.0

    old_minimizer = new_minimizer
    old_old_minimizer = new_minimizer
    old_minimum = new_minimum
    old_old_minimum = new_minimum

    iteration = 0
    is_converged = false

    while iteration < iterations
        p = 0.0
        q = 0.0

        x_tol = rtol * abs(new_minimizer) + atol
        x_midpoint = (x_upper + x_lower) / 2

        if abs(new_minimizer - x_midpoint) <= 2 * x_tol - (x_upper - x_lower) / 2
            is_converged = true
            break
        end

        iteration += 1

        if abs(old_step) > x_tol
            r = (new_minimizer - old_minimizer) * (new_minimum - old_old_minimum)
            q = (new_minimizer - old_old_minimizer) * (new_minimum - old_minimum)
            p =
                (new_minimizer - old_old_minimizer) * q -
                (new_minimizer - old_minimizer) * r
            q = 2 * (q - r)

            if q > 0
                p = -p
            else
                q = -q
            end
        end

        if abs(p) < abs(q * old_step / 2) &&
           p < q * (x_upper - new_minimizer) &&
           p < q * (new_minimizer - x_lower)
            old_step = step
            step = p / q

            x_temp = new_minimizer + step
            if (x_temp - x_lower) < 2 * x_tol || (x_upper - x_temp) < 2 * x_tol
                step = new_minimizer < x_midpoint ? x_tol : -x_tol
            end
        else
            old_step =
                new_minimizer < x_midpoint ? x_upper - new_minimizer :
                x_lower - new_minimizer
            step = golden_ratio * old_step
        end

        if abs(step) >= x_tol
            new_x = new_minimizer + step
        else
            new_x = new_minimizer + (step > 0 ? x_tol : -x_tol)
        end

        new_f = f(new_x)
        fcalls += 1

        if new_f < new_minimum
            if new_x < new_minimizer
                x_upper = new_minimizer
            else
                x_lower = new_minimizer
            end
            old_old_minimizer = old_minimizer
            old_old_minimum = old_minimum
            old_minimizer = new_minimizer
            old_minimum = new_minimum
            new_minimizer = new_x
            new_minimum = new_f
        else
            if new_x < new_minimizer
                x_lower = new_x
            else
                x_upper = new_x
            end
            if new_f <= old_minimum || old_minimizer == new_minimizer
                old_old_minimizer = old_minimizer
                old_old_minimum = old_minimum
                old_minimizer = new_x
                old_minimum = new_f
            elseif new_f <= old_old_minimum ||
                   old_old_minimizer == new_minimizer ||
                   old_old_minimizer == old_minimizer
                old_old_minimizer = new_x
                old_old_minimum = new_f
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
