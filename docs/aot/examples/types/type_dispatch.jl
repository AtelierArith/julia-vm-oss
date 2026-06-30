# Multiple Dispatch Examples
#
# このファイルは “複数メソッド” の雰囲気を出すため、型ごとに関数を分けた例。
#（本来の Julia の multiple dispatch を厳密に再現するというより、AoT で型が
# 固まった関数を増やすと速くなる、という観点のサンプル）

# ============================================================================
# Basic Multiple Dispatch
# ============================================================================

"""
    add_int(a::Int64, b::Int64)::Int64

Integer addition.
"""
function add_int(a::Int64, b::Int64)::Int64
    return a + b
end

"""
    add_float(a::Float64, b::Float64)::Float64

Float addition.
"""
function add_float(a::Float64, b::Float64)::Float64
    return a + b
end

"""
    add_mixed_int_float(a::Int64, b::Float64)::Float64

Mixed type addition (promotes to Float64).
"""
function add_mixed_int_float(a::Int64, b::Float64)::Float64
    return Float64(a) + b
end

"""
    add_mixed_float_int(a::Float64, b::Int64)::Float64

Mixed type addition (promotes to Float64).
"""
function add_mixed_float_int(a::Float64, b::Int64)::Float64
    return a + Float64(b)
end

# ============================================================================
# Type-Specific Algorithms
# ============================================================================

"""
    square_int(x::Int64)::Int64

Square an integer.
"""
function square_int(x::Int64)::Int64
    return x * x
end

"""
    square_float(x::Float64)::Float64

Square a float.
"""
function square_float(x::Float64)::Float64
    return x * x
end

"""
    cube_int(x::Int64)::Int64

Cube an integer.
"""
function cube_int(x::Int64)::Int64
    return x * x * x
end

"""
    cube_float(x::Float64)::Float64

Cube a float.
"""
function cube_float(x::Float64)::Float64
    return x * x * x
end

# ============================================================================
# Container-Based Operations
# ============================================================================

"""
    sum_elements_int(arr)::Int64

Sum integer array elements.
"""
function sum_elements_int(arr::Vector{Int64})::Int64
    total = 0
    for x in arr
        total += x
    end
    return total
end

"""
    sum_elements_float(arr)::Float64

Sum float array elements.
"""
function sum_elements_float(arr::Vector{Float64})::Float64
    total = 0.0
    for x in arr
        total += x
    end
    return total
end

# ============================================================================
# Numeric Type Conversion
# ============================================================================

"""
    int_to_float64(x::Int64)::Float64

Convert integer to float.
"""
function int_to_float64(x::Int64)::Float64
    return Float64(x)
end

"""
    identity_float64(x::Float64)::Float64

Identity for floats.
"""
function identity_float64(x::Float64)::Float64
    return x
end

# ============================================================================
# Comparison Operations
# ============================================================================

"""
    max_int(a::Int64, b::Int64)::Int64

Maximum of two integers.
"""
function max_int(a::Int64, b::Int64)::Int64
    if a > b
        return a
    else
        return b
    end
end

"""
    max_float(a::Float64, b::Float64)::Float64

Maximum of two floats.
"""
function max_float(a::Float64, b::Float64)::Float64
    if a > b
        return a
    else
        return b
    end
end

function main()
    add_int(1, 2)
    add_float(1.0, 2.0)
    add_mixed_int_float(1, 2.0)
    add_mixed_float_int(1.0, 2)
    square_int(5)
    square_float(2.5)
    cube_int(3)
    cube_float(2.0)
    sum_elements_int([1, 2, 3, 4, 5])
    sum_elements_float([1.0, 2.0, 3.0])
    int_to_float64(42)
    identity_float64(3.14)
    max_int(3, 5)
    max_float(3.0, 5.0)
    true
end
