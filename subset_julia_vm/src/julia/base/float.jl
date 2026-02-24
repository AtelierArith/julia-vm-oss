# =============================================================================
# float.jl - Floating-Point Arithmetic
# =============================================================================
# Based on Julia's base/float.jl
# Defines arithmetic and comparison operators for floating-point numbers using intrinsics.

# =============================================================================
# Floating-Point Arithmetic
# =============================================================================

# Unary minus
function Base.:(-)(x::Float64)
    neg_float(x)
end

# Binary subtraction
function Base.:(-)(x::Float64, y::Float64)
    sub_float(x, y)
end

# Binary addition
function Base.:(+)(x::Float64, y::Float64)
    add_float(x, y)
end

# Multiplication
function Base.:(*)(x::Float64, y::Float64)
    mul_float(x, y)
end

# Division
function Base.:(/)(x::Float64, y::Float64)
    div_float(x, y)
end

# Power
function Base.:(^)(x::Float64, y::Float64)
    pow_float(x, y)
end

# =============================================================================
# Floating-Point Comparisons
# =============================================================================

# Less than
function Base.:(<)(x::Float64, y::Float64)
    lt_float(x, y)
end

# Less or equal
function Base.:(<=)(x::Float64, y::Float64)
    le_float(x, y)
end

# Greater than
function Base.:(>)(x::Float64, y::Float64)
    gt_float(x, y)
end

# Greater or equal
function Base.:(>=)(x::Float64, y::Float64)
    ge_float(x, y)
end

# Equality
function Base.:(==)(x::Float64, y::Float64)
    eq_float(x, y)
end

# Not equal
function Base.:(!=)(x::Float64, y::Float64)
    ne_float(x, y)
end

# =============================================================================
# Sign-related functions for Float64 (based on Julia's base/float.jl)
# =============================================================================

# signbit for Float64 - checks if the sign bit is set
# Based on Julia's base/floatfuncs.jl:15
# Note: In Julia Base, this uses bitcast, but we use a simpler implementation
# that handles negative zero correctly by checking if 1/x is negative infinity
function signbit(x::Float64)
    # For negative zero, x < 0.0 returns false, but 1.0/x returns -Inf
    # This handles: -0.0, negative numbers, and -Inf correctly
    if x < 0.0
        return true
    elseif x == 0.0
        # Check for negative zero: 1.0/-0.0 = -Inf
        return (1.0 / x) < 0.0
    else
        return false
    end
end

# abs for Float64 - uses abs_float intrinsic
# Based on Julia's base/float.jl:698
function abs(x::Float64)
    return abs_float(x)
end

# =============================================================================
# Float32 Arithmetic (for type preservation)
# =============================================================================

# Unary minus for Float32
function Base.:(-)(x::Float32)
    Float32(neg_float(Float64(x)))
end

# Binary addition for Float32
function Base.:(+)(x::Float32, y::Float32)
    Float32(add_float(Float64(x), Float64(y)))
end

# Binary subtraction for Float32
function Base.:(-)(x::Float32, y::Float32)
    Float32(sub_float(Float64(x), Float64(y)))
end

# Multiplication for Float32
function Base.:(*)(x::Float32, y::Float32)
    Float32(mul_float(Float64(x), Float64(y)))
end

# Division for Float32
function Base.:(/)(x::Float32, y::Float32)
    Float32(div_float(Float64(x), Float64(y)))
end

# Power for Float32
function Base.:(^)(x::Float32, y::Float32)
    Float32(pow_float(Float64(x), Float64(y)))
end

# =============================================================================
# Float32 Mixed-Type Arithmetic
# =============================================================================
# Mixed-type operations (e.g., Float32 + Int64) are now handled by the
# promotion-based Number fallback operators in promotion.jl:
#   +(x::Number, y::Number) = let (px, py) = promote(x, y); px + py; end
#
# The explicit methods that were here have been removed because:
# 1. The promotion system correctly handles all mixed-type combinations
# 2. Method dispatch specificity ensures concrete-type methods always win
# 3. This matches Julia's official implementation pattern
# =============================================================================

# =============================================================================
# Float32 Comparisons
# =============================================================================

# Less than for Float32
function Base.:(<)(x::Float32, y::Float32)
    lt_float(Float64(x), Float64(y))
end

# Less or equal for Float32
function Base.:(<=)(x::Float32, y::Float32)
    le_float(Float64(x), Float64(y))
end

# Greater than for Float32
function Base.:(>)(x::Float32, y::Float32)
    gt_float(Float64(x), Float64(y))
end

# Greater or equal for Float32
function Base.:(>=)(x::Float32, y::Float32)
    ge_float(Float64(x), Float64(y))
end

# Equality for Float32
function Base.:(==)(x::Float32, y::Float32)
    eq_float(Float64(x), Float64(y))
end

# Not equal for Float32
function Base.:(!=)(x::Float32, y::Float32)
    ne_float(Float64(x), Float64(y))
end
