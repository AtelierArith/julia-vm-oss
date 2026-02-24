# =============================================================================
# rounding.jl - Rounding Mode Types
# =============================================================================
# Based on Julia's base/rounding.jl
#
# Defines RoundingMode type and constants for controlling floating point
# rounding behavior.

# =============================================================================
# RoundingMode Type
# =============================================================================

"""
    RoundingMode

A type used for controlling the rounding mode of floating point operations
(via `round` function), or as optional arguments for rounding to the nearest
integer.

Currently supported rounding modes are:

- [`RoundNearest`](@ref) (default)
- [`RoundNearestTiesAway`](@ref)
- [`RoundNearestTiesUp`](@ref)
- [`RoundToZero`](@ref)
- [`RoundFromZero`](@ref)
- [`RoundUp`](@ref)
- [`RoundDown`](@ref)

Note: This is a simplified implementation for SubsetJuliaVM. In official Julia,
RoundingMode is a parametric type `RoundingMode{T}` where T is a Symbol.
Here we use a struct with a Symbol field for compatibility.
"""
struct RoundingMode
    mode::Symbol
end

# =============================================================================
# RoundingMode Constants
# =============================================================================

"""
    RoundNearest

The default rounding mode. Rounds to the nearest integer, with ties (fractional
values of 0.5) being rounded to the nearest even integer.
"""
const RoundNearest = RoundingMode(:Nearest)

"""
    RoundToZero

[`round`](@ref) using this rounding mode is an alias for [`trunc`](@ref).
"""
const RoundToZero = RoundingMode(:ToZero)

"""
    RoundUp

[`round`](@ref) using this rounding mode is an alias for [`ceil`](@ref).
"""
const RoundUp = RoundingMode(:Up)

"""
    RoundDown

[`round`](@ref) using this rounding mode is an alias for [`floor`](@ref).
"""
const RoundDown = RoundingMode(:Down)

"""
    RoundFromZero

Rounds away from zero.
"""
const RoundFromZero = RoundingMode(:FromZero)

"""
    RoundNearestTiesAway

Rounds to nearest integer, with ties rounded away from zero (C/C++
[`round`](@ref) behaviour).
"""
const RoundNearestTiesAway = RoundingMode(:NearestTiesAway)

"""
    RoundNearestTiesUp

Rounds to nearest integer, with ties rounded toward positive infinity
(Java/JavaScript [`round`](@ref) behaviour).
"""
const RoundNearestTiesUp = RoundingMode(:NearestTiesUp)
