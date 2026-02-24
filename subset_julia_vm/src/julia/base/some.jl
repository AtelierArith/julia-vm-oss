# =============================================================================
# some.jl - Optional value wrapper
# =============================================================================
# Based on Julia's base/some.jl
#
# Some{T} wraps a value to distinguish between:
# - nothing (absence of value)
# - Some(nothing) (presence of nothing as a value)
#
# Use something() to unwrap Some values.

struct Some
    value
end

# =============================================================================
# something - return first non-nothing value
# =============================================================================
# Based on Julia's base/some.jl
# Unwraps Some values, returns first non-nothing

function something(x)
    if isa(x, Some)
        return x.value
    end
    if x !== nothing
        return x
    end
    error("ArgumentError: No value other than `nothing` found")
end

function something(x, y)
    if isa(x, Some)
        return x.value
    end
    if x !== nothing
        return x
    end
    if isa(y, Some)
        return y.value
    end
    if y !== nothing
        return y
    end
    error("ArgumentError: No value other than `nothing` found")
end

function something(x, y, z)
    if isa(x, Some)
        return x.value
    end
    if x !== nothing
        return x
    end
    if isa(y, Some)
        return y.value
    end
    if y !== nothing
        return y
    end
    if isa(z, Some)
        return z.value
    end
    if z !== nothing
        return z
    end
    error("ArgumentError: No value other than `nothing` found")
end

function something(x, y, z, w)
    if isa(x, Some)
        return x.value
    end
    if x !== nothing
        return x
    end
    if isa(y, Some)
        return y.value
    end
    if y !== nothing
        return y
    end
    if isa(z, Some)
        return z.value
    end
    if z !== nothing
        return z
    end
    if isa(w, Some)
        return w.value
    end
    if w !== nothing
        return w
    end
    error("ArgumentError: No value other than `nothing` found")
end
