# =============================================================================
# Operators - Pairwise comparison operations
# =============================================================================
# Based on Julia's base/operators.jl

# min: return the smaller of two values
function min(x, y)
    if x < y
        return x
    else
        return y
    end
end

# min: variadic form — reduce pairwise (Issue #2135)
function min(x, y, args...)
    m = min(x, y)
    for a in args
        m = min(m, a)
    end
    return m
end

# max: return the larger of two values
function max(x, y)
    if x > y
        return x
    else
        return y
    end
end

# max: variadic form — reduce pairwise (Issue #2135)
function max(x, y, args...)
    m = max(x, y)
    for a in args
        m = max(m, a)
    end
    return m
end

# minmax: return (min, max) as a tuple
function minmax(x, y)
    if x < y
        return (x, y)
    else
        return (y, x)
    end
end

# copysign: return x with the sign of y
# Based on Julia's base/number.jl:252
function copysign(x, y)
    if signbit(y)
        return -abs(x)
    else
        return abs(x)
    end
end

# Note: flipsign is now defined in number.jl as the generic fallback
# and in int.jl for Int64

# =============================================================================
# Comparison functions
# =============================================================================

# cmp: three-way comparison, returns -1, 0, or 1
function cmp(x, y)
    if x < y
        return -1
    elseif x > y
        return 1
    else
        return 0
    end
end

# isless: strict weak ordering comparison (handles NaN correctly)
# NaN is considered greater than all other values
function isless(x, y)
    # Check if x is NaN
    if x != x
        return false
    end
    # Check if y is NaN
    if y != y
        return true
    end
    return x < y
end

# isequal: equality comparison (NaN == NaN is true, unlike ==)
function isequal(x, y)
    # Both NaN
    if x != x && y != y
        return true
    end
    return x == y
end

# Float64 specialization: uses === (bit-identical comparison)
function isequal(a::Float64, b::Float64)
    return a === b
end

# Note: Additional isequal specializations (Nothing, Missing, Int64<->Float64,
# Array, Tuple, Expr) are in missing.jl to avoid function index limit in this file.

# isapprox: approximate equality (≈)
# Default tolerance: rtol=sqrt(eps), atol=0
# For scalars: uses abs for comparison
# For arrays: uses L2 norm (requires isa check at runtime)
function isapprox(x, y)
    # Use relative tolerance of ~1.5e-8 (sqrt of machine epsilon)
    rtol = 1.4901161193847656e-8
    atol = 0.0
    # Check if inputs are arrays
    if isa(x, Array) && isa(y, Array)
        return _isapprox_array(x, y, rtol, atol)
    else
        return _isapprox_scalar(x, y, rtol, atol)
    end
end

# isapprox with custom tolerances
function isapprox(x, y, rtol, atol)
    # Check if inputs are arrays
    if isa(x, Array) && isa(y, Array)
        return _isapprox_array(x, y, rtol, atol)
    else
        return _isapprox_scalar(x, y, rtol, atol)
    end
end

# Internal scalar implementation
function _isapprox_scalar(x, y, rtol, atol)
    return abs(x - y) <= max(atol, rtol * max(abs(x), abs(y)))
end

# isapprox for arrays: uses L2 norm
# This is called from LinearAlgebra module after checking isa(x, Array)
# Supports both real and complex arrays using abs() for magnitude
# Computes element-wise differences to avoid needing array - operator
function _isapprox_array(x, y, rtol, atol)
    # Check lengths match
    n = length(x)
    if n != length(y)
        return false
    end

    # Compute L2 norm of difference element by element
    s = 0.0
    for i in 1:n
        d = x[i] - y[i]  # Element-wise subtraction (scalar - scalar)
        # Use abs()^2 to handle complex values correctly
        ad = abs(d)
        s = s + ad * ad
    end
    diff_norm = sqrt(s)

    # Compute norm of x
    sx = 0.0
    for i in 1:n
        v = x[i]
        av = abs(v)
        sx = sx + av * av
    end
    norm_x = sqrt(sx)

    # Compute norm of y
    sy = 0.0
    for i in 1:n
        v = y[i]
        av = abs(v)
        sy = sy + av * av
    end
    norm_y = sqrt(sy)

    max_norm = max(norm_x, norm_y)
    return diff_norm <= max(atol, rtol * max_norm)
end

# =============================================================================
# Identity operators
# =============================================================================

# !== (≢): not identical (negation of ===)
# Based on Julia's base/operators.jl
# !==(a, b) is equivalent to !(a === b)
function !==(a, b)
    return !(a === b)
end

# =============================================================================
# Type equality
# =============================================================================
# Based on Julia's base/operators.jl:295-297
# In Julia, Type equality uses ccall(:jl_types_equal). For SubsetJuliaVM,
# we use identity comparison which is correct for DataType values.

==(T::Type, S::Type) = T === S
!=(T::Type, S::Type) = !(T === S)

# Unicode aliases for identity operators (≡ and ≢)
# Based on Julia's base/operators.jl:348,370
# These operators are handled directly in the lowering:
#   ≡ is lowered to === (object identity)
#   ≢ is lowered to !== (not identical)
# Export declarations are in exports.jl

# =============================================================================
# Type widening
# =============================================================================
# Based on Julia's base/operators.jl and base/int.jl

# widen: return a type one step wider than the argument
# For fixed-size integer types less than 64 bits, widen will return a wider type.

# Type-based widen: returns the widened type
widen(::Type{Int8}) = Int16
widen(::Type{Int16}) = Int32
widen(::Type{Int32}) = Int64
widen(::Type{Int64}) = Int64  # Can't widen Int64 further without Int128
widen(::Type{Float32}) = Float64
widen(::Type{Float64}) = Float64  # Can't widen Float64 further without BigFloat

# Value-based widen: converts value to widened type
widen(x::Int8) = convert(Int16, x)
widen(x::Int16) = convert(Int32, x)
widen(x::Int32) = convert(Int64, x)
widen(x::Int64) = x  # Already at maximum supported width
widen(x::Float32) = convert(Float64, x)
widen(x::Float64) = x  # Already at maximum supported width

# =============================================================================
# Identity function
# =============================================================================
# Based on Julia's base/operators.jl:584

# identity: return the argument unchanged
# This is useful as a "do nothing" function argument, or as a default function parameter.
identity(x) = x

# =============================================================================
# Pipe operator
# =============================================================================
# Based on Julia's base/operators.jl:980

# |>: infix operator which applies function f to argument x
# This allows f(g(x)) to be written as x |> g |> f
|>(x, f) = f(x)

# =============================================================================
# isunordered - check if value is unordered (NaN, Missing)
# =============================================================================
# Based on Julia's base/operators.jl:291-293
# Returns true for values where comparisons are undefined.
# NaN and missing are unordered (comparisons with them don't follow total order).
# Note: Missing specializations are in missing.jl (isunordered(::Missing) = true)

isunordered(x) = false
isunordered(x::Float64) = isnan(x)

# =============================================================================
# isgreater - Descending total order comparison
# =============================================================================
# Based on Julia's base/operators.jl:277
#
# isgreater(x, y) tests whether x is greater than y according to a fixed total
# order compatible with min. NaN and missing are ordered as smallest values.
# This is NOT the inverse of isless.

function isgreater(x, y)
    if isunordered(x) || isunordered(y)
        return isless(x, y)
    else
        return isless(y, x)
    end
end

# =============================================================================
# Modular arithmetic - mod1, fld1, fldmod1
# =============================================================================
# Based on Julia's base/operators.jl:893-930

# mod1: modulus after flooring division, returning a value in (0, y]
# Unlike mod(x, y) which returns values in [0, y), mod1(x, y) returns values in (0, y]
# mod1(4, 2) = 2 (not 0)
# mod1(3, 3) = 3 (not 0)
function mod1(x::Int64, y::Int64)
    m = mod(x, y)
    if m == 0
        return y
    else
        return m
    end
end

function mod1(x::Float64, y::Float64)
    m = mod(x, y)
    if m == 0.0
        return y
    else
        return m
    end
end

# fld1: flooring division, returning a value consistent with mod1(x, y)
# Based on Julia's base/operators.jl:917-921
# The relationship: x == (fld1(x, y) - 1) * y + mod1(x, y)
function fld1(x::Int64, y::Int64)
    # Use Float64 version for simplicity
    m = mod1(x, y)
    return fld((x - m) + y, y)
end

function fld1(x::Float64, y::Float64)
    m = mod1(x, y)
    return fld((x - m) + y, y)
end

# fldmod1: return both fld1 and mod1 as a tuple
# Based on Julia's base/operators.jl:930
fldmod1(x, y) = (fld1(x, y), mod1(x, y))

# =============================================================================
# Returns - functor that returns a constant value
# =============================================================================
# Based on Julia's base/operators.jl
#
# Returns(x) creates a callable that always returns x
# Useful for HOFs: filter(Returns(true), arr) keeps all elements

struct Returns
    value
end

# Make Returns callable - always returns the stored value
# Note: This requires special handling in the VM for callable structs
# For now, we define a helper function
function call_returns(r::Returns)
    return r.value
end
