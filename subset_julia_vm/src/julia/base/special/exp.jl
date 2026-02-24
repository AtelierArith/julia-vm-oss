# =============================================================================
# Exponential function — Pure Julia implementation
# =============================================================================
# Based on FDLIBM (Freely Distributable LibM) by Sun Microsystems
# Reference: julia/base/special/exp.jl

# Constants
const _LN2_HI = 6.93147180369123816490e-01
const _LN2_LO = 1.90821492927058500170e-10
const _LOG2E  = 1.44269504088896338700e+00

# =============================================================================
# exp(x::Float64)
# =============================================================================
function exp(x::Float64)
    if x != x
        return x
    end
    if x - x != 0.0
        if x > 0.0
            return x
        else
            return 0.0
        end
    end

    if x > 709.782712893384
        return 1.0 / 0.0
    end
    if x < -745.1332191019411
        return 0.0
    end

    if abs(x) < 2.220446049250313e-16
        return 1.0 + x
    end

    k = Int64(round(x * _LOG2E))
    fk = Float64(k)
    r = x - fk * _LN2_HI - fk * _LN2_LO

    # Taylor series for exp(r), |r| ≤ ln(2)/2, 13 terms.
    # Keep Horner evaluation expanded in steps to avoid very deep AST nesting.
    p = 2.08767569878681e-9
    p = 2.505210838544172e-8 + r * p
    p = 2.7557319223985888e-7 + r * p
    p = 2.7557319223985893e-6 + r * p
    p = 2.48015873015873e-5 + r * p
    p = 1.984126984126984e-4 + r * p
    p = 1.388888888888889e-3 + r * p
    p = 8.333333333333333e-3 + r * p
    p = 4.1666666666666664e-2 + r * p
    p = 1.6666666666666666e-1 + r * p
    p = 0.5 + r * p
    p = 1.0 + r * p
    p = 1.0 + r * p

    if k > 1023
        p = p * (2.0 ^ 1023)
        k = k - 1023
        if k > 1023
            return 1.0 / 0.0
        end
        return p * (2.0 ^ k)
    elseif k < -1074
        return 0.0
    elseif k < -1021
        p = p * (2.0 ^ (-1021))
        k = k + 1021
        return p * (2.0 ^ k)
    else
        return p * (2.0 ^ k)
    end
end

# Integer conversion
exp(x::Int64) = exp(Float64(x))
