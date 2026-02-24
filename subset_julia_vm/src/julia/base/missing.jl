# =============================================================================
# Missing - Missing value support
# =============================================================================
# Based on Julia's base/missing.jl
#
# The Missing type represents missing data in statistical and data analysis contexts.
# Unlike `nothing` (absence of value), `missing` represents unknown or unavailable data.

# ismissing: check if value is missing
function ismissing(x)
    return x === missing
end

# coalesce: return the first non-missing value
# coalesce(x, y) returns x if x is not missing, otherwise y
function coalesce(x, y)
    if ismissing(x)
        return y
    else
        return x
    end
end

# coalesce with 3 arguments
function coalesce(x, y, z)
    if ismissing(x)
        if ismissing(y)
            return z
        else
            return y
        end
    else
        return x
    end
end

# coalesce with 4 arguments
function coalesce(x, y, z, w)
    if ismissing(x)
        if ismissing(y)
            if ismissing(z)
                return w
            else
                return z
            end
        else
            return y
        end
    else
        return x
    end
end

# =============================================================================
# isequal for Missing
# =============================================================================
# isequal returns Bool (not missing), which is important for sorting and hashing.
# Two missing values are considered equal (isequal(missing, missing) = true),
# but missing is not equal to any other value.

# isequal(::Missing, ::Missing) = true
function isequal(x::Missing, y::Missing)
    return true
end

# isequal(::Missing, ::Any) = false
function isequal(x::Missing, y)
    return false
end

# isequal(::Any, ::Missing) = false
function isequal(x, y::Missing)
    return false
end

# =============================================================================
# isless for Missing
# =============================================================================
# isless defines a total order for sorting. Missing values sort to the end
# (are considered greater than all other values).
# isless returns Bool (not missing).

# isless(::Missing, ::Missing) = false (missing is not less than itself)
function isless(x::Missing, y::Missing)
    return false
end

# isless(::Missing, ::Any) = false (missing is not less than anything)
function isless(x::Missing, y)
    return false
end

# isless(::Any, ::Missing) = true (everything is less than missing)
function isless(x, y::Missing)
    return true
end

# =============================================================================
# isequal for Missing (Issue #2718)
# =============================================================================
function isequal(a::Missing, b::Missing)
    return true
end
function isequal(a::Missing, b)
    return false
end
function isequal(a, b::Missing)
    return false
end

# Nothing specialization for isequal (Issue #2718)
function isequal(a::Nothing, b::Nothing)
    return true
end

# Cross-type numeric specializations for isequal (Issue #2718)
function isequal(a::Int64, b::Float64)
    return isequal(Float64(a), b)
end
function isequal(a::Float64, b::Int64)
    return isequal(a, Float64(b))
end

# Array specialization: element-wise isequal with shape check (Issue #2718)
function isequal(A::Array, B::Array)
    if length(A) != length(B)
        return false
    end
    if ndims(A) != ndims(B)
        return false
    end
    for d in 1:ndims(A)
        if size(A, d) != size(B, d)
            return false
        end
    end
    for i in 1:length(A)
        if isequal(A[i], B[i]) == false
            return false
        end
    end
    return true
end

# Tuple specialization: element-wise isequal (Issue #2718)
function isequal(t1::Tuple, t2::Tuple)
    if length(t1) != length(t2)
        return false
    end
    for i in 1:length(t1)
        if isequal(t1[i], t2[i]) == false
            return false
        end
    end
    return true
end

# Expr specialization: structural comparison via === (Issue #2718)
function isequal(a::Expr, b::Expr)
    return a === b
end

# =============================================================================
# isunordered for Missing (Issue #2715)
# =============================================================================
# Missing values are unordered — comparisons with missing are undefined.
# Based on Julia's base/operators.jl:293
isunordered(x::Missing) = true

# =============================================================================
# Note on ispositive, isnegative, isapprox, min, and max for Missing
# =============================================================================
# In Julia, these functions should return `missing` for Missing values:
#   ispositive(::Missing) = missing
#   isnegative(::Missing) = missing
#   isapprox(::Missing, ::Any) = missing
#   min(::Missing, ::Any) = missing
#   max(::Missing, ::Any) = missing
#
# However, the current method dispatch system doesn't properly select
# type-specific methods (x::Missing) over generic methods (x).
# This requires additional work on the method dispatch system (Issue #719).
#
# For now, comparison operators (==, <, >, etc.) with literal `missing` values
# are handled at compile-time in binary.rs.
