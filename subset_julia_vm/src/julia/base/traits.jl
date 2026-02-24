# =============================================================================
# traits.jl - Numeric and Object Traits
# =============================================================================
# Based on Julia's base/traits.jl
#
# Trait types for describing properties of numeric types and ranges.

# =============================================================================
# OrderStyle - trait for objects that have an ordering
# =============================================================================

"""
    OrderStyle

Abstract type for traits describing whether a type has a natural ordering.
"""
abstract type OrderStyle end

"""
    Ordered <: OrderStyle

Trait indicating that a type has a natural ordering (supports `<`, `>`, etc.).
Examples: `Real`, `AbstractString`, `Symbol`.
"""
struct Ordered <: OrderStyle end

"""
    Unordered <: OrderStyle

Trait indicating that a type does not have a natural ordering.
Examples: `Complex`, custom types without comparison operators.
"""
struct Unordered <: OrderStyle end

# =============================================================================
# ArithmeticStyle - trait for objects that support arithmetic
# =============================================================================

"""
    ArithmeticStyle

Abstract type for traits describing the arithmetic behavior of numeric types.
"""
abstract type ArithmeticStyle end

"""
    ArithmeticRounds <: ArithmeticStyle

Trait indicating that arithmetic operations may lose least significant bits
due to rounding. Examples: floating-point types.
"""
struct ArithmeticRounds <: ArithmeticStyle end

"""
    ArithmeticWraps <: ArithmeticStyle

Trait indicating that arithmetic operations may lose most significant bits
due to overflow wrapping. Examples: fixed-width integer types.
"""
struct ArithmeticWraps <: ArithmeticStyle end

"""
    ArithmeticUnknown <: ArithmeticStyle

Trait indicating unknown arithmetic behavior. Used as default for types
that don't fit other categories.
"""
struct ArithmeticUnknown <: ArithmeticStyle end

# =============================================================================
# RangeStepStyle - trait for range step regularity
# =============================================================================

"""
    RangeStepStyle

Abstract type for traits describing whether a range type has regular steps.

A regular step means that `step(r)` will always be exactly equal to the
difference between two subsequent elements in the range.
"""
abstract type RangeStepStyle end

"""
    RangeStepRegular <: RangeStepStyle

Trait indicating that a range type has perfectly regular steps.
This allows O(1) hashing algorithms for ranges.
"""
struct RangeStepRegular <: RangeStepStyle end

"""
    RangeStepIrregular <: RangeStepStyle

Trait indicating that a range type may have irregular steps due to
rounding errors. This is the default for non-integer element types.
"""
struct RangeStepIrregular <: RangeStepStyle end
