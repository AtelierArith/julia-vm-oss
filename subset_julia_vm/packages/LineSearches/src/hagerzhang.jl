# HagerZhang line search (Issue #8059).
#
# Faithful pure-Julia port of upstream LineSearches `src/hagerzhang.jl`:
#   W. W. Hager and H. Zhang (2006), Algorithm 851: CG_DESCENT.  ACM TOMS 32.
#
# This is the default line search for `Optim.BFGS()`.  It checks the (approximate)
# Wolfe conditions, brackets a minimizer along the search direction, and refines
# the bracket with secant interpolation falling back to bisection.  The driver is
# written as a plain function (`hagerzhang_search`) rather than a callable struct
# so it composes cleanly on the no-JIT VM; the `HagerZhang` struct only carries
# the tuning parameters.  Upstream's bitfield display tracing and `@assert`
# invariant checks are omitted; the numeric trajectory is unchanged.

"""
    HagerZhang(; delta = 0.1, sigma = 0.9, alphamax = Inf, rho = 5.0,
                 epsilon = 1e-6, gamma = 0.66, linesearchmax = 50, psi3 = 0.1)

Hager-Zhang approximate-Wolfe line search parameters.  Defaults reproduce
upstream `LineSearches.HagerZhang()`, the default line search of `Optim.BFGS`.
"""
struct HagerZhang
    delta::Float64
    sigma::Float64
    alphamax::Float64
    rho::Float64
    epsilon::Float64
    gamma::Float64
    linesearchmax::Int
    psi3::Float64
end
HagerZhang(;
    delta = 0.1,
    sigma = 0.9,
    alphamax = Inf,
    rho = 5.0,
    epsilon = 1e-6,
    gamma = 0.66,
    linesearchmax = 50,
    psi3 = 0.1,
) = HagerZhang(
    Float64(delta),
    Float64(sigma),
    Float64(alphamax),
    Float64(rho),
    Float64(epsilon),
    Float64(gamma),
    Int(linesearchmax),
    Float64(psi3),
)

"""
    LineSearchException(message, alpha)

Raised when the line search cannot satisfy the Wolfe conditions.  `alpha` carries
the best step found so the caller (`perform_linesearch!`) can exit gracefully.
"""
struct LineSearchException
    message::String
    alpha::Float64
end

# Wolfe (and approximate-Wolfe) test, HZ eqs (T1)/(T2).
function _hz_satisfies_wolfe(c, phi_c, dphi_c, phi_0, dphi_0, phi_lim, delta, sigma)
    wolfe1 = delta * dphi_0 >= (phi_c - phi_0) / c && dphi_c >= sigma * dphi_0
    wolfe2 = (2 * delta - 1) * dphi_0 >= dphi_c >= sigma * dphi_0 && phi_c <= phi_lim
    return wolfe1 || wolfe2
end

# HZ secant step (stages S1-S4).
_hz_secant(a, b, da, db) = (a * db - b * da) / (db - da)
_hz_secant_idx(al, sl, ia, ib) = _hz_secant(al[ia], al[ib], sl[ia], sl[ib])

# HZ update U0-U3: given a third point, keep the two that bracket the minimizer.
function _hz_update!(phidphi, alphas, values, slopes, ia, ib, ic, phi_lim, phi_0, dphi_0, delta, sigma)
    a = alphas[ia]
    b = alphas[ib]
    c = alphas[ic]
    phi_c = values[ic]
    dphi_c = slopes[ic]
    if c < a || c > b
        return ia, ib, false
    end
    if dphi_c >= 0.0
        return ia, ic, false
    end
    if phi_c <= phi_lim
        return ic, ib, false
    end
    return _hz_bisect!(phidphi, alphas, values, slopes, ia, ic, phi_lim, phi_0, dphi_0, delta, sigma)
end

# HZ stage U3 bisection (theta = 0.5).
function _hz_bisect!(phidphi, alphas, values, slopes, ia, ib, phi_lim, phi_0, dphi_0, delta, sigma)
    a = alphas[ia]
    b = alphas[ib]
    while b - a > eps(b)
        d = (a + b) / 2
        phi_d, gphi = phidphi(d)
        push!(alphas, d)
        push!(values, phi_d)
        push!(slopes, gphi)
        id = length(alphas)
        if _hz_satisfies_wolfe(d, phi_d, gphi, phi_0, dphi_0, phi_lim, delta, sigma)
            return ia, id, true
        end
        if gphi >= 0.0
            return ia, id, false
        end
        if phi_d <= phi_lim
            a = d
            ia = id
        else
            b = d
            ib = id
        end
    end
    return ia, ib, false
end

# HZ secant^2 (stage S).
function _hz_secant2!(phidphi, alphas, values, slopes, ia, ib, phi_lim, delta, sigma)
    phi_0 = values[1]
    dphi_0 = slopes[1]
    a = alphas[ia]
    b = alphas[ib]
    dphi_a = slopes[ia]
    dphi_b = slopes[ib]
    c = _hz_secant(a, b, dphi_a, dphi_b)
    phi_c, dphi_c = phidphi(c)
    push!(alphas, c)
    push!(values, phi_c)
    push!(slopes, dphi_c)
    ic = length(alphas)
    if _hz_satisfies_wolfe(c, phi_c, dphi_c, phi_0, dphi_0, phi_lim, delta, sigma)
        return true, ic, ic
    end
    iA, iB, iswolfe = _hz_update!(phidphi, alphas, values, slopes, ia, ib, ic, phi_lim, phi_0, dphi_0, delta, sigma)
    if iswolfe
        return true, iB, iB
    end
    a = alphas[iA]
    b = alphas[iB]
    if iB == ic
        c = _hz_secant_idx(alphas, slopes, ib, iB)
    elseif iA == ic
        c = _hz_secant_idx(alphas, slopes, ia, iA)
    end
    if (iA == ic || iB == ic) && a <= c <= b
        phi_c, dphi_c = phidphi(c)
        push!(alphas, c)
        push!(values, phi_c)
        push!(slopes, dphi_c)
        ic = length(alphas)
        if _hz_satisfies_wolfe(c, phi_c, dphi_c, phi_0, dphi_0, phi_lim, delta, sigma)
            return true, ic, ic
        end
        iA, iB, iswolfe = _hz_update!(phidphi, alphas, values, slopes, iA, iB, ic, phi_lim, phi_0, dphi_0, delta, sigma)
        if iswolfe
            return true, iB, iB
        end
    end
    return false, iA, iB
end

"""
    hagerzhang_search(ls::HagerZhang, phidphi, c, phi_0, dphi_0) -> (alpha, phi_alpha)

Run the Hager-Zhang line search.  `phidphi(alpha)` returns `(phi(alpha), phi'(alpha))`
where `phi(alpha) = f(x + alpha*s)` and `phi'(alpha) = dot(grad f(x + alpha*s), s)`.
`c` is the initial step (from the `alphaguess`).  Throws `LineSearchException` if
the Wolfe conditions cannot be met within `linesearchmax` iterations.
"""
function hagerzhang_search(ls::HagerZhang, phidphi, c, phi_0, dphi_0)
    delta = ls.delta
    sigma = ls.sigma
    alphamax = ls.alphamax
    rho = ls.rho
    epsilon = ls.epsilon
    gamma = ls.gamma
    linesearchmax = ls.linesearchmax
    psi3 = ls.psi3

    if !(isfinite(phi_0) && isfinite(dphi_0))
        throw(LineSearchException("Value and slope at step length = 0 must be finite.", 0.0))
    end
    if dphi_0 >= eps(Float64) * abs(phi_0)
        throw(LineSearchException("Search direction is not a direction of descent.", 0.0))
    elseif dphi_0 >= 0.0
        return 0.0, phi_0
    end

    iterfinitemax = ceil(Int, -log2(eps(Float64)))
    alphas = [0.0]
    values = [phi_0]
    slopes = [dphi_0]
    phi_lim = phi_0 + epsilon * abs(phi_0)
    c <= eps(Float64) && return 0.0, phi_0

    phi_c, dphi_c = phidphi(c)
    iterfinite = 1
    while !(isfinite(phi_c) && isfinite(dphi_c)) && iterfinite < iterfinitemax
        iterfinite += 1
        c *= psi3
        phi_c, dphi_c = phidphi(c)
    end
    if !(isfinite(phi_c) && isfinite(dphi_c))
        return 0.0, phi_0
    end
    push!(alphas, c)
    push!(values, phi_c)
    push!(slopes, dphi_c)

    # Bracketing (HZ stages B0-B3).
    isbracketed = false
    ia = 1
    ib = 2
    iter = 1
    cold = -1.0
    phi_cold = phi_0
    while !isbracketed && iter < linesearchmax
        if dphi_c >= 0.0
            ib = length(alphas)
            for i = (ib-1):-1:1
                if values[i] <= phi_lim
                    ia = i
                    break
                end
            end
            isbracketed = true
        elseif values[end] > phi_lim
            ib = length(alphas)
            ia = 1
            ia, ib, iswolfe = _hz_bisect!(phidphi, alphas, values, slopes, ia, ib, phi_lim, phi_0, dphi_0, delta, sigma)
            if iswolfe
                return alphas[ib], values[ib]
            end
            isbracketed = true
        else
            cold = c
            phi_cold = phi_c
            if nextfloat(cold) >= alphamax
                return cold, phi_cold
            end
            c *= rho
            if c > alphamax
                c = alphamax
            end
            phi_c, dphi_c = phidphi(c)
            iterfinite = 1
            while !(isfinite(phi_c) && isfinite(dphi_c)) && c > nextfloat(cold) && iterfinite < iterfinitemax
                alphamax = c
                iterfinite += 1
                c = (cold + c) / 2
                phi_c, dphi_c = phidphi(c)
            end
            if !(isfinite(phi_c) && isfinite(dphi_c))
                return cold, phi_cold
            end
            push!(alphas, c)
            push!(values, phi_c)
            push!(slopes, dphi_c)
            if _hz_satisfies_wolfe(c, phi_c, dphi_c, phi_0, dphi_0, phi_lim, delta, sigma)
                return c, phi_c
            end
        end
        iter += 1
    end

    # Refinement (secant^2, falling back to bisection).
    while iter < linesearchmax
        a = alphas[ia]
        b = alphas[ib]
        if b - a <= eps(b)
            return a, values[ia]
        end
        iswolfe, iA, iB = _hz_secant2!(phidphi, alphas, values, slopes, ia, ib, phi_lim, delta, sigma)
        if iswolfe
            return alphas[iA], values[iA]
        end
        A = alphas[iA]
        B = alphas[iB]
        if B - A < gamma * (b - a)
            ia = iA
            ib = iB
        else
            c = (A + B) / 2
            phi_c, dphi_c = phidphi(c)
            push!(alphas, c)
            push!(values, phi_c)
            push!(slopes, dphi_c)
            if _hz_satisfies_wolfe(c, phi_c, dphi_c, phi_0, dphi_0, phi_lim, delta, sigma)
                return c, phi_c
            end
            ia, ib, iswolfe = _hz_update!(phidphi, alphas, values, slopes, iA, iB, length(alphas), phi_lim, phi_0, dphi_0, delta, sigma)
            if iswolfe
                return alphas[ib], values[ib]
            end
        end
        iter += 1
    end

    best_i = 1
    best_v = values[1]
    for i in eachindex(values)
        if values[i] < best_v
            best_v = values[i]
            best_i = i
        end
    end
    throw(LineSearchException("Linesearch failed to converge, reached maximum iterations.", alphas[best_i]))
end
