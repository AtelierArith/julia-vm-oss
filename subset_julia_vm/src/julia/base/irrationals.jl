# =============================================================================
# Irrationals - Irrational mathematical constants
# =============================================================================
# Based on Julia's base/irrationals.jl
#
# Provides the Irrational{sym} type for exact representation of irrational
# constants like pi, e, etc. with automatic conversion to Float64.
#
# NOTE: Type predicates (isfinite, isinteger, iszero, isone) are intentionally
# not defined here to avoid dispatch conflicts with the builtin methods.
# Users can define their own predicates for their custom irrational types.

# AbstractIrrational <: Real - base type for irrational values
abstract type AbstractIrrational <: Real end

# Irrational{sym} - parametric struct for specific irrational constants
struct Irrational{sym} <: AbstractIrrational end
