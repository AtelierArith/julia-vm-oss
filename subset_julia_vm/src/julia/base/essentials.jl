# =============================================================================
# essentials.jl - Essential language support functions
# =============================================================================
# Based on Julia's base/essentials.jl

# =============================================================================
# ifelse - conditional without short-circuit evaluation
# =============================================================================
# Based on Julia's base/essentials.jl
#
# ifelse(condition, x, y) evaluates both x and y, returns x if true, y if false
# Unlike ternary operator, both branches are always evaluated

function ifelse(condition::Bool, x, y)
    if condition
        return x
    else
        return y
    end
end

# =============================================================================
# oftype - convert to type of reference value
# =============================================================================
# Based on Julia's base/essentials.jl
#
# oftype(x, y) converts y to the type of x

function oftype(x, y)
    return convert(typeof(x), y)
end
