# =============================================================================
# MathConstants - Mathematical constants
# =============================================================================
# Based on Julia's base/mathconstants.jl
#
# Mathematical constants are defined as Float64 values for simplicity.
# Julia's official implementation uses the Irrational type for higher precision,
# but SubsetJuliaVM uses Float64 for simplicity and compatibility.
#
# IMPORTANT: Only π, pi, and ℯ are exported from Base.
# Other constants (e, γ, eulergamma, φ, golden, catalan) are only available
# via Base.MathConstants module, matching upstream Julia behavior.

# Top-level constants exported from Base (see julia/base/exports.jl)
# π (pi) - ratio of circumference to diameter
const π = 3.141592653589793
const pi = π

# ℯ (Euler's number) - base of natural logarithm
const ℯ = 2.718281828459045

# =============================================================================
# MathConstants module - organized access to mathematical constants
# =============================================================================
# Usage: using Base.MathConstants or Base.MathConstants.π

"""
    Base.MathConstants

Module containing the mathematical constants.
See [`π`](@ref), [`ℯ`](@ref), [`γ`](@ref), [`φ`](@ref) and [`catalan`](@ref).

# Examples
```julia
julia> using Base.MathConstants

julia> π
3.141592653589793

julia> MathConstants.golden
1.618033988749895
```
"""
module MathConstants

export π, pi, ℯ, e, γ, eulergamma, catalan, φ, golden

# π (pi) - ratio of circumference to diameter
# The constant π is the ratio of a circle's circumference to its diameter.
const π = 3.141592653589793
const pi = π

# ℯ (Euler's number) - base of natural logarithm
# Also known as Napier's constant.
const ℯ = 2.718281828459045
const e = ℯ

# γ (gamma) - Euler-Mascheroni constant
# The limiting difference between the harmonic series and natural logarithm.
const γ = 0.5772156649015329
const eulergamma = γ

# φ (phi) - golden ratio
# The ratio (1 + √5) / 2 ≈ 1.618...
const φ = 1.618033988749895
const golden = φ

# catalan - Catalan's constant
# Defined as the alternating sum of reciprocals of odd squares.
const catalan = 0.9159655941772190

end # module MathConstants
