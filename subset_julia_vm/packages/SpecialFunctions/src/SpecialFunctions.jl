module SpecialFunctions

export gamma, loggamma, logabsgamma, beta, lbeta, beta_inc, gamma_inc, erf, erfc, erfi, digamma, trigamma, zeta, eta

const _lanczos_g = 7.0
const _lanczos_p = (
    0.99999999999980993,
    676.5203681218851,
    -1259.1392167224028,
    771.32342877765313,
    -176.61502916214059,
    12.507343278686905,
    -0.13857109526572012,
    9.9843695780195716e-6,
    1.5056327351493116e-7,
)

function _gamma_lanczos(z::Float64)
    # Reflection formula for z < 0.5
    if z < 0.5
        pi_z = pi * z
        return pi / (sin(pi_z) * _gamma_lanczos(1.0 - z))
    end
    z -= 1.0
    x = _lanczos_p[1]
    for i in 1:8
        x += _lanczos_p[i + 1] / (z + Float64(i))
    end
    t = z + _lanczos_g + 0.5
    return sqrt(2.0 * pi) * t^(z + 0.5) * exp(-t) * x
end

gamma(x::Real) = _gamma_lanczos(Float64(x))

function loggamma(x::Real)
    x = Float64(x)
    if x <= 0.0
        error("loggamma not defined for x <= 0")
    end
    return log(gamma(x))
end

function logabsgamma(x::Real)
    x = Float64(x)
    if x > 0.0
        return (loggamma(x), 1)
    else
        return (loggamma(1.0 - x), -1)
    end
end

beta(a::Real, b::Real) = gamma(a) * gamma(b) / gamma(a + b)
lbeta(a::Real, b::Real) = loggamma(a) + loggamma(b) - loggamma(a + b)

# erf(x) = sign(x) * P(1/2, x^2), where P is the regularized lower incomplete
# gamma function. This reuses the high-accuracy `gamma_inc` series/continued
# fraction (≈1e-12) instead of the ~1e-7 Abramowitz & Stegun polynomial, which
# matters for Normal/LogNormal cdf accuracy (Issue #7178).
function erf(x::Real)
    x = Float64(x)
    if x == 0.0
        return 0.0
    elseif x > 0.0
        return gamma_inc(0.5, x * x)
    else
        return -gamma_inc(0.5, x * x)
    end
end

# erfc(x) = Q(1/2, x^2) for x >= 0; use the upper tail directly so large-x
# values keep full relative accuracy instead of cancelling against erf(x) ≈ 1.
function erfc(x::Real)
    x = Float64(x)
    if x >= 0.0
        return x * x >= 0.5 + 1.0 ? _gamma_inc_cf(0.5, x * x) : 1.0 - gamma_inc(0.5, x * x)
    else
        return 1.0 + gamma_inc(0.5, x * x)
    end
end
erfi(x::Real) = error("erfi requires Complex support (Issue #7178 Phase 5)")

# Digamma/trigamma: deferred to Phase 2+; Distributions.jl only needs them
# for entropy of Beta/Dirichlet and some advanced fitters.
digamma(x::Real) = error("digamma not yet implemented (Issue #7178 Phase 2)")
trigamma(x::Real) = error("trigamma not yet implemented (Issue #7178 Phase 2)")

function _beta_inc_cf(a::Float64, b::Float64, x::Float64, maxiter::Int=200, eps::Float64=1e-10)
    am = 1.0
    bm = 1.0
    az = 1.0
    qab = a + b
    qap = a + 1.0
    qam = a - 1.0
    bz = 1.0 - qab * x / qap
    for m in 1:maxiter
        m2 = 2 * m
        d = m * (b - m) * x / ((qam + m2) * (a + m2))
        ap = az + d * am
        bp = bz + d * bm
        d = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        app = ap + d * az
        bpp = bp + d * bz
        aold = az
        am = ap / bpp
        bm = bp / bpp
        az = app / bpp
        bz = 1.0
        if abs(az - aold) < eps * abs(az)
            return az
        end
    end
    error("beta_inc continued fraction did not converge")
end

function beta_inc(a::Real, b::Real, x::Real)
    a = Float64(a); b = Float64(b); x = Float64(x)
    if x < 0.0 || x > 1.0
        error("x must be in [0,1]")
    end
    if x == 0.0
        return 0.0
    elseif x == 1.0
        return 1.0
    end
    # Prefactor x^a (1-x)^b / B(a,b); lbeta = log(B(a,b)), hence the negative sign.
    bt = exp(-lbeta(a, b) + a * log(x) + b * log(1.0 - x))
    if x < (a + 1.0) / (a + b + 2.0)
        return bt * _beta_inc_cf(a, b, x) / a
    else
        return 1.0 - bt * _beta_inc_cf(b, a, 1.0 - x) / b
    end
end

# Regularized lower incomplete gamma P(a,x) via the series expansion
# (Numerical Recipes `gser`), valid for x < a+1.
function _gamma_inc_series(a::Float64, x::Float64, maxiter::Int=200, eps::Float64=1e-12)
    if x <= 0.0
        return 0.0
    end
    ap = a
    sum = 1.0 / a
    del = sum
    for _ in 1:maxiter
        ap += 1.0
        del *= x / ap
        sum += del
        if abs(del) < abs(sum) * eps
            return sum * exp(-x + a * log(x) - loggamma(a))
        end
    end
    error("gamma_inc series did not converge")
end

# Regularized upper incomplete gamma Q(a,x) via the continued fraction
# (Numerical Recipes `gcf`), valid for x >= a+1. P(a,x) = 1 - Q(a,x).
function _gamma_inc_cf(a::Float64, x::Float64, maxiter::Int=200, eps::Float64=1e-12)
    tiny = 1e-30
    b = x + 1.0 - a
    c = 1.0 / tiny
    d = 1.0 / b
    h = d
    for i in 1:maxiter
        an = -i * (i - a)
        b += 2.0
        d = an * d + b
        if abs(d) < tiny
            d = tiny
        end
        c = b + an / c
        if abs(c) < tiny
            c = tiny
        end
        d = 1.0 / d
        del = d * c
        h *= del
        if abs(del - 1.0) < eps
            return exp(-x + a * log(x) - loggamma(a)) * h
        end
    end
    error("gamma_inc continued fraction did not converge")
end

# Regularized lower incomplete gamma P(a,x) = γ(a,x) / Γ(a).
# (Subset convention: returns the scalar regularized value, like `beta_inc`.)
function gamma_inc(a::Real, x::Real)
    a = Float64(a); x = Float64(x)
    if x < 0.0 || a <= 0.0
        error("gamma_inc requires a > 0 and x >= 0")
    end
    if x == 0.0
        return 0.0
    end
    if x < a + 1.0
        return _gamma_inc_series(a, x)
    else
        return 1.0 - _gamma_inc_cf(a, x)
    end
end

# Riemann zeta function ζ(s) for real s. Ported from SpecialFunctions' `_zeta`
# (the Float64 path of its Hurwitz-zeta-derived algorithm, specialized to z==1).
# Complex `s` is out of scope here, matching the rest of this subset module
# (cf. `erfi`, Issue #7178 Phase 5).

# Bernoulli/Stirling asymptotic-series coefficients used by `_zeta_pg_horner`.
const _zeta_pg_coeffs = (
    0.08333333333333333,
    -0.008333333333333333,
    0.003968253968253968,
    -0.004166666666666667,
    0.007575757575757576,
    -0.021092796092796094,
    0.08333333333333333,
    -0.4432598039215686,
    3.0539543302701198,
)

# Horner-style evaluation of the polygamma asymptotic series, equivalent to
# SpecialFunctions' `@pg_horner(x, m, p...)` macro:
#   c[1] * (p[1] + d[2]*x*(p[2] + d[3]*x*(p[3] + ...)))
# with c[1] = m+1 and d[k] = (2k+m-1)*(2k+m-2) / ((2k-1)*(2k-2)).
function _zeta_pg_horner(x::Float64, m::Float64)
    p = _zeta_pg_coeffs
    ex = (m + 17.0) * (m + 16.0) * (p[9] / (17.0 * 16.0))
    for k in 8:-1:2
        d = 2 * k
        cdiv = 1.0 / Float64((d - 1) * (d - 2))
        ex = (cdiv * (m + Float64(d - 1)) * (m + Float64(d - 2))) * (p[k] + x * ex)
    end
    return (m + 1.0) * (p[1] + x * ex)
end

# Taylor series of ζ(s) around s = 0 for small |s| (SpecialFunctions @evalpoly).
function _zeta_taylor_small(s::Float64)
    c0 = -0.5
    c1 = -0.918938533204672741780329736405617639861
    c2 = -1.0031782279542924256050500133649802190
    c3 = -1.00078519447704240796017680222772921424
    c4 = -0.9998792995005711649578008136558752359121
    return c0 + s * (c1 + s * (c2 + s * (c3 + s * c4)))
end

function _zeta_real(s::Float64)
    # Pole at s = 1.
    if s == 1.0
        return NaN
    end
    # Non-finite inputs: ζ(NaN)=NaN, ζ(+Inf)=1, ζ(-Inf)=NaN (match SpecialFunctions).
    if isnan(s)
        return NaN
    end
    if !isfinite(s)
        return s > 0.0 ? 1.0 : NaN
    end

    if s < 0.5
        # Taylor expansion for small |s| (avoids cancellation in the reflection).
        if abs(s) < 1.0e-3
            return _zeta_taylor_small(s)
        end
        # Reflection: ζ(s) = ζ(1-s) * Γ(1-s) * sinpi(s/2) * (2π)^s / π.
        return _zeta_real(1.0 - s) * gamma(1.0 - s) * sinpi(s * 0.5) * (2.0 * pi)^s / pi
    end

    m = s - 1.0
    # For real s the empirical Stirling-series cutoff reduces to 6 (imag part is 0).
    n = 6
    acc = 1.0
    for nu in 2:n
        acc_old = acc
        acc += (1.0 / Float64(nu))^s
        if acc == acc_old
            break
        end
    end
    z = Float64(1 + n)
    t = 1.0 / z
    w = t^m
    acc += w * (1.0 / m + 0.5 * t)
    t = t * t  # 1/z^2
    acc += w * t * _zeta_pg_horner(t, m)
    return acc
end

zeta(s::Real) = _zeta_real(Float64(s))

# Generalized (Hurwitz) zeta function ζ(s, z) for real s and z. Ported from
# upstream `SpecialFunctions._zeta(s, z)` (the Float64 path of the algorithm,
# which is the m-th derivative of the digamma asymptotic expansion). Complex
# arguments remain out of scope (cf. `erfi`, Issue #7178 Phase 5).
function _hurwitz_zeta(s::Float64, z::Float64)
    # z == 1 (Riemann) and z == 0 (k=0 term excluded) both reduce to ζ(s).
    if z == 1.0 || z == 0.0
        return _zeta_real(s)
    end
    if isnan(s) || isnan(z)
        return NaN
    end

    x = z  # real(z)

    # s = Inf: distance to the poles determines 0 vs Inf.
    if !isfinite(s)
        if s == Inf
            far = false
            if x >= 0.5
                far = abs(z) > 1.0
            else
                far = abs(z - round(x)) > 1.0
            end
            if x > 1.0 || far
                return 0.0  # distance to poles is > 1
            end
            if x > 0.0
                return Inf
            end
        end
        # Nothing clever to return for -Inf.
        return NaN
    end

    m = s - 1.0
    acc = 0.0

    # Shift z past the asymptotic-series cutoff using the recurrence formula.
    # For real arguments the cutoff reduces to 7 + (s - 1) = 6 + s.
    cutoff = 7.0 + m
    if x < cutoff
        xf = floor(x)
        nx = Int64(xf)
        n = Int64(ceil(cutoff - xf))
        minus_s = -s
        if nx < 0
            # z < 0: use the (-z) recurrence so every power has a positive base.
            minus_z = -z
            acc += minus_z^minus_s  # ν = 0 term
            if xf != z
                acc += (z - Float64(nx))^minus_s
            end
            # Loop largest→smallest (s > 0) or smallest→largest (s ≤ 0) so the
            # running sum stays accurate and can halt once terms stop mattering.
            if s > 0.0
                nu = -nx - 1
                while nu >= 1
                    acc_old = acc
                    acc += (minus_z - Float64(nu))^minus_s
                    if acc == acc_old
                        break
                    end
                    nu -= 1
                end
            else
                nu = 1
                while nu <= -nx - 1
                    acc_old = acc
                    acc += (minus_z - Float64(nu))^minus_s
                    if acc == acc_old
                        break
                    end
                    nu += 1
                end
            end
        else
            # x ≥ 0 && z != 0
            acc += z^minus_s
        end

        # Main recurrence sum over ν in [max(1, 1 - nx), n - 1].
        lo = 1 - nx
        if lo < 1
            lo = 1
        end
        if s > 0.0
            nu = lo
            while nu <= n - 1
                acc_old = acc
                acc += (z + Float64(nu))^minus_s
                if acc == acc_old
                    break
                end
                nu += 1
            end
        else
            nu = n - 1
            while nu >= lo
                acc_old = acc
                acc += (z + Float64(nu))^minus_s
                if acc == acc_old
                    break
                end
                nu -= 1
            end
        end
        z = z + Float64(n)
    end

    t = 1.0 / z
    w = t^m
    acc += w * (1.0 / m + 0.5 * t)
    t = t * t  # 1/z^2
    acc += w * t * _zeta_pg_horner(t, m)
    return acc
end

zeta(s::Real, z::Real) = _hurwitz_zeta(Float64(s), Float64(z))

# Dirichlet eta function η(s) = Σ (-1)^(n-1) / n^s, expressed via the Riemann
# zeta as η(s) = -ζ(s) * expm1(log(2) * (1 - s)), with a Taylor branch near s = 1
# (η(1) = log 2). Ported from upstream `SpecialFunctions._eta`.
function _eta_real(z::Float64)
    dz = 1.0 - z
    if abs(dz) < 7.0e-3
        # Taylor expansion around z == 1.
        return 0.6931471805599453094172321214581765 *
               evalpoly(dz, (1.0,
                             -0.23064207462156020589789602935331414700440,
                             -0.047156357547388879740146103148112380421254,
                             -0.002263576552598880778433550956278702759143568,
                             0.001081837223249910136105931217561387128141157))
    else
        return -_zeta_real(z) * expm1(0.6931471805599453094172321214581765 * dz)
    end
end

eta(s::Real) = _eta_real(Float64(s))

end # module SpecialFunctions
