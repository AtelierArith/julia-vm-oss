# =============================================================================
# Logarithmic function — Pure Julia implementation
# =============================================================================
# Based on FDLIBM (Freely Distributable LibM) by Sun Microsystems
# Reference: julia/base/special/log.jl

# Constants
const _LOG_LN2_HI = 6.93147180369123816490e-01
const _LOG_LN2_LO = 1.90821492927058500170e-10
const _LOG_SQRT2  = 0.7071067811865476

# =============================================================================
# log(x::Float64)
# =============================================================================
function log(x::Float64)
    if x != x
        return x
    end
    if x < 0.0
        return 0.0 / 0.0
    end
    if x == 0.0
        return -1.0 / 0.0
    end
    if x - x != 0.0
        return x
    end
    if x == 1.0
        return 0.0
    end

    m, k = frexp(x)
    if m < _LOG_SQRT2
        m = m * 2.0
        k = k - 1
    end

    f = m - 1.0
    fk = Float64(k)

    if abs(f) < 1.0e-10
        return fk * _LOG_LN2_HI + fk * _LOG_LN2_LO + f
    end

    s = f / (2.0 + f)
    z = s * s

    # FDLIBM minimax polynomial
    R = z * (6.666666666666735130e-01 + z * (3.999999999940941908e-01 + z * (2.857142874366239149e-01 + z * (2.222219843214978396e-01 + z * (1.818357216161805012e-01 + z * (1.531383769920937332e-01 + z * 1.479819860511658591e-01))))))

    hfsq = 0.5 * f * f

    return fk * _LOG_LN2_HI - ((hfsq - (s * (hfsq + R) + fk * _LOG_LN2_LO)) - f)
end

# Integer conversion
log(x::Int64) = log(Float64(x))
