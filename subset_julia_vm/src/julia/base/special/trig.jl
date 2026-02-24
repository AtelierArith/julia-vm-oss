# =============================================================================
# Trigonometric functions — Pure Julia implementations
# =============================================================================
# Based on FDLIBM (Freely Distributable LibM) by Sun Microsystems
# Reference: julia/base/special/trig.jl
#
# Functions: sin, cos, tan, asin, acos, atan (1-arg and 2-arg)

# =============================================================================
# Constants for range reduction
# =============================================================================
const _PIO2_HI = 1.5707963267948966
const _PIO2_LO = 6.123233995736766e-17
const _TWOOPI  = 0.6366197723675814

# atan reference values (high + low parts)
const _ATAN_HI_0 = 4.63647609000806093515e-01
const _ATAN_LO_0 = 2.26987774529616870924e-17
const _ATAN_HI_1 = 7.85398163397448278999e-01
const _ATAN_LO_1 = 3.06161699786838301793e-17
const _ATAN_HI_2 = 9.82793723247329054082e-01
const _ATAN_LO_2 = 1.39033110312309984516e-17
const _ATAN_HI_3 = 1.57079632679489655800e+00
const _ATAN_LO_3 = 6.12323399573676603587e-17

# =============================================================================
# Internal kernels
# =============================================================================

# sin kernel: minimax polynomial for |x| ≤ π/4
# Coefficients from FDLIBM (Sun Microsystems)
function _sin_kernel(x::Float64)
    x2 = x * x
    r = x2 * (-1.66666666666666324348e-01 + x2 * (8.33333333332248946124e-03 + x2 * (-1.98412698298579493134e-04 + x2 * (2.75573137070700676789e-06 + x2 * (-2.50507602534068634195e-08 + x2 * 1.58969099521155010221e-10)))))
    return x + x * r
end

# cos kernel: minimax polynomial for |x| ≤ π/4
# Coefficients from FDLIBM (Sun Microsystems)
function _cos_kernel(x::Float64)
    x2 = x * x
    r = x2 * x2 * (4.16666666666666019037e-02 + x2 * (-1.38888888888741095749e-03 + x2 * (2.48015872894767294178e-05 + x2 * (-2.75573143513906633035e-07 + x2 * (2.08757232129817482790e-09 + x2 * (-1.13596475577881948265e-11))))))
    return 1.0 - 0.5 * x2 + r
end

# Range reduction: reduce x to [-π/4, π/4]
function _rem_pio2(x::Float64)
    ax = abs(x)
    if ax <= 0.7853981633974483
        return (0, x)
    end
    n = Int64(round(x * _TWOOPI))
    fn = Float64(n)
    r = x - fn * _PIO2_HI - fn * _PIO2_LO
    return (n, r)
end

# =============================================================================
# sin(x::Float64)
# =============================================================================
function sin(x::Float64)
    if x != x
        return x
    end
    if x == 0.0
        return x
    end
    if x - x != 0.0
        return 0.0 / 0.0
    end

    n, r = _rem_pio2(x)
    m = n % 4
    if m < 0
        m = m + 4
    end

    if m == 0
        return _sin_kernel(r)
    elseif m == 1
        return _cos_kernel(r)
    elseif m == 2
        return -_sin_kernel(r)
    else
        return -_cos_kernel(r)
    end
end

# =============================================================================
# cos(x::Float64)
# =============================================================================
function cos(x::Float64)
    if x != x
        return x
    end
    if x - x != 0.0
        return 0.0 / 0.0
    end

    n, r = _rem_pio2(x)
    m = n % 4
    if m < 0
        m = m + 4
    end

    if m == 0
        return _cos_kernel(r)
    elseif m == 1
        return -_sin_kernel(r)
    elseif m == 2
        return -_cos_kernel(r)
    else
        return _sin_kernel(r)
    end
end

# =============================================================================
# tan(x::Float64)
# =============================================================================
function tan(x::Float64)
    return sin(x) / cos(x)
end

# =============================================================================
# atan polynomial kernel
# =============================================================================
function _atan_poly(x::Float64)
    z = x * x
    w = z * z
    s1 = z * (3.33333333333329318027e-01 + w * (1.42857142725034663711e-01 + w * (9.09090909090613630636e-02 + w * (6.66666666066797879830e-02 + w * (5.25398036241779040070e-02 + w * 3.53553390593099085390e-02)))))
    s2 = w * (-1.99999999998764832476e-01 + w * (-1.11111104054623557880e-01 + w * (-7.69230769050185273009e-02 + w * (-5.88235294347717846150e-02 + w * (-4.44444442946313483720e-02)))))
    return x - x * (s1 + s2)
end

# =============================================================================
# atan(x::Float64)
# =============================================================================
function atan(x::Float64)
    if x != x
        return x
    end
    if x == 0.0
        return x
    end

    neg = x < 0.0
    ax = abs(x)

    if ax - ax != 0.0
        result = _ATAN_HI_3 + _ATAN_LO_3
        return neg ? -result : result
    end

    if ax > 1.0e16
        result = _ATAN_HI_3 + _ATAN_LO_3
        return neg ? -result : result
    end

    if ax < 0.4375
        if ax < 3.7252902984619140625e-09
            return x
        end
        result = _atan_poly(ax)
    elseif ax < 0.6875
        t = (2.0 * ax - 1.0) / (2.0 + ax)
        result = _ATAN_HI_0 + (_ATAN_LO_0 + _atan_poly(t))
    elseif ax < 1.1875
        t = (ax - 1.0) / (ax + 1.0)
        result = _ATAN_HI_1 + (_ATAN_LO_1 + _atan_poly(t))
    elseif ax < 2.4375
        t = (ax - 1.5) / (1.0 + 1.5 * ax)
        result = _ATAN_HI_2 + (_ATAN_LO_2 + _atan_poly(t))
    else
        t = -1.0 / ax
        result = _ATAN_HI_3 + (_ATAN_LO_3 + _atan_poly(t))
    end

    return neg ? -result : result
end

# =============================================================================
# atan(y, x) — two-argument (atan2)
# =============================================================================
function atan(y::Float64, x::Float64)
    if y != y || x != x
        return 0.0 / 0.0
    end

    pi_val = 3.141592653589793
    pi_o_2 = 1.5707963267948966
    pi_o_4 = 0.7853981633974483

    if y == 0.0
        if x > 0.0
            return y
        elseif x < 0.0
            return copysign(pi_val, y)
        else
            return copysign(pi_val, y)
        end
    end
    if x == 0.0
        return y > 0.0 ? pi_o_2 : -pi_o_2
    end

    y_inf = (y - y != 0.0)
    x_inf = (x - x != 0.0)
    if y_inf && x_inf
        if x > 0.0
            return y > 0.0 ? pi_o_4 : -pi_o_4
        else
            return y > 0.0 ? 3.0 * pi_o_4 : -3.0 * pi_o_4
        end
    end

    if y_inf
        return y > 0.0 ? pi_o_2 : -pi_o_2
    end

    if x_inf
        if x > 0.0
            return copysign(0.0, y)
        else
            return copysign(pi_val, y)
        end
    end

    r = atan(abs(y / x))
    if x > 0.0
        return y > 0.0 ? r : -r
    else
        return y > 0.0 ? pi_val - r : -(pi_val - r)
    end
end

# =============================================================================
# asin(x::Float64)
# =============================================================================
function asin(x::Float64)
    if x != x
        return x
    end
    ax = abs(x)
    if ax > 1.0
        return 0.0 / 0.0
    end
    if ax == 1.0
        return x > 0.0 ? 1.5707963267948966 : -1.5707963267948966
    end
    if ax < 1.0e-8
        return x
    end
    return atan(x / sqrt(1.0 - x * x))
end

# =============================================================================
# acos(x::Float64)
# =============================================================================
function acos(x::Float64)
    if x != x
        return x
    end
    if abs(x) > 1.0
        return 0.0 / 0.0
    end
    return 1.5707963267948966 - asin(x)
end

# =============================================================================
# Integer conversion methods
# =============================================================================
sin(x::Int64) = sin(Float64(x))
cos(x::Int64) = cos(Float64(x))
tan(x::Int64) = tan(Float64(x))
asin(x::Int64) = asin(Float64(x))
acos(x::Int64) = acos(Float64(x))
atan(x::Int64) = atan(Float64(x))
atan(y::Int64, x::Int64) = atan(Float64(y), Float64(x))
