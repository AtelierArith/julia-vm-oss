# =============================================================================
# Exponential function — Pure Julia implementation
# =============================================================================
# Based on FDLIBM (Freely Distributable LibM) by Sun Microsystems
# Reference: julia/base/special/exp.jl

# Constants
const _LN2_HI = 6.93147180369123816490e-01
const _LN2_LO = 1.90821492927058500170e-10
const _LOG2E  = 1.44269504088896338700e+00

function _exp2_int_float64(k::Int64)
    if k > 1023
        return 1.0 / 0.0
    elseif k < -1074
        return 0.0
    elseif k >= -1022
        return reinterpret(Float64, UInt64(k + 1023) << 52)
    else
        return reinterpret(Float64, UInt64(1) << (k + 1074))
    end
end

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
        p = p * _exp2_int_float64(1023)
        k = k - 1023
        if k > 1023
            return 1.0 / 0.0
        end
        return p * _exp2_int_float64(k)
    elseif k < -1075
        return 0.0
    elseif k < -1021
        p = p * _exp2_int_float64(-1021)
        k = k + 1021
        return p * _exp2_int_float64(k)
    else
        return p * _exp2_int_float64(k)
    end
end

function exp(x::Float32)
    return Float32(exp(Float64(x)))
end

function exp(x::Float16)
    return Float16(exp(Float64(x)))
end

# Integer conversion
exp(x::Int64) = exp(Float64(x))
exp(x::Bool) = exp(Float64(x))

# Rational conversion (Issue #5356): without an explicit method, `exp(::Rational)`
# is lenient-dispatched to `exp(::Float64)` and the Rational lands in a Float64
# slot (LoadSlotF64 InternalError). `float(::Rational)` is Float64, so this
# reduces to the Float64 method with no recursion.
exp(x::Rational) = exp(float(x))
