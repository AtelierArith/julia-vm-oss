# =============================================================================
# boot.jl - Core.Intrinsics wrappers
# =============================================================================
# Based on Julia's base/boot.jl
# These functions wrap Core.Intrinsics and are recognized by the compiler
# to emit direct CallIntrinsic instructions.

# =============================================================================
# Integer Arithmetic
# =============================================================================

# neg_int(x) -> -x
neg_int(x) = Core.Intrinsics.neg_int(x)

# add_int(a, b) -> a + b
add_int(x, y) = Core.Intrinsics.add_int(x, y)

# sub_int(a, b) -> a - b
sub_int(x, y) = Core.Intrinsics.sub_int(x, y)

# mul_int(a, b) -> a * b
mul_int(x, y) = Core.Intrinsics.mul_int(x, y)

# sdiv_int(a, b) -> a / b (signed division, truncated toward zero)
sdiv_int(x, y) = Core.Intrinsics.sdiv_int(x, y)

# srem_int(a, b) -> a % b (signed remainder)
srem_int(x, y) = Core.Intrinsics.srem_int(x, y)

# =============================================================================
# Integer Comparisons
# =============================================================================

# eq_int(a, b) -> a == b
eq_int(x, y) = Core.Intrinsics.eq_int(x, y)

# ne_int(a, b) -> a != b
ne_int(x, y) = Core.Intrinsics.ne_int(x, y)

# slt_int(a, b) -> a < b (signed less than)
slt_int(x, y) = Core.Intrinsics.slt_int(x, y)

# sle_int(a, b) -> a <= b (signed less or equal)
sle_int(x, y) = Core.Intrinsics.sle_int(x, y)

# sgt_int(a, b) -> a > b (signed greater than)
sgt_int(x, y) = Core.Intrinsics.sgt_int(x, y)

# sge_int(a, b) -> a >= b (signed greater or equal)
sge_int(x, y) = Core.Intrinsics.sge_int(x, y)

# =============================================================================
# Floating-Point Arithmetic
# =============================================================================

# neg_float(x) -> -x
neg_float(x::Float64) = Core.Intrinsics.neg_float(x)

# add_float(a, b) -> a + b
add_float(x::Float64, y::Float64) = Core.Intrinsics.add_float(x, y)

# sub_float(a, b) -> a - b
sub_float(x::Float64, y::Float64) = Core.Intrinsics.sub_float(x, y)

# mul_float(a, b) -> a * b
mul_float(x::Float64, y::Float64) = Core.Intrinsics.mul_float(x, y)

# div_float(a, b) -> a / b
div_float(x::Float64, y::Float64) = Core.Intrinsics.div_float(x, y)

# pow_float(a, b) -> a ^ b
pow_float(x::Float64, y::Float64) = Core.Intrinsics.pow_float(x, y)

# =============================================================================
# Floating-Point Comparisons
# =============================================================================

# eq_float(a, b) -> a == b
eq_float(x::Float64, y::Float64) = Core.Intrinsics.eq_float(x, y)

# ne_float(a, b) -> a != b
ne_float(x::Float64, y::Float64) = Core.Intrinsics.ne_float(x, y)

# lt_float(a, b) -> a < b
lt_float(x::Float64, y::Float64) = Core.Intrinsics.lt_float(x, y)

# le_float(a, b) -> a <= b
le_float(x::Float64, y::Float64) = Core.Intrinsics.le_float(x, y)

# gt_float(a, b) -> a > b
gt_float(x::Float64, y::Float64) = Core.Intrinsics.gt_float(x, y)

# ge_float(a, b) -> a >= b
ge_float(x::Float64, y::Float64) = Core.Intrinsics.ge_float(x, y)

# =============================================================================
# Bitwise Operations
# =============================================================================

# and_int(a, b) -> a & b
and_int(x, y) = Core.Intrinsics.and_int(x, y)

# or_int(a, b) -> a | b
or_int(x, y) = Core.Intrinsics.or_int(x, y)

# xor_int(a, b) -> a ^ b (xor)
xor_int(x, y) = Core.Intrinsics.xor_int(x, y)

# not_int(x) -> ~x
not_int(x) = Core.Intrinsics.not_int(x)

# shl_int(a, b) -> a << b (shift left)
shl_int(x, y) = Core.Intrinsics.shl_int(x, y)

# lshr_int(a, b) -> a >>> b (logical shift right)
lshr_int(x, y) = Core.Intrinsics.lshr_int(x, y)

# ashr_int(a, b) -> a >> b (arithmetic shift right)
ashr_int(x, y) = Core.Intrinsics.ashr_int(x, y)

# =============================================================================
# Type Conversions
# =============================================================================

# sitofp(x) -> convert signed int to float
sitofp(x) = Core.Intrinsics.sitofp(x)

# fptosi(x) -> convert float to signed int (truncate)
fptosi(x) = Core.Intrinsics.fptosi(x)

# =============================================================================
# Low-Level Math (CPU/FPU instructions)
# =============================================================================

# sqrt_llvm(x) -> sqrt(x)
sqrt_llvm(x) = Core.Intrinsics.sqrt_llvm(x)

# floor_llvm(x) -> floor(x)
floor_llvm(x) = Core.Intrinsics.floor_llvm(x)

# ceil_llvm(x) -> ceil(x)
ceil_llvm(x) = Core.Intrinsics.ceil_llvm(x)

# trunc_llvm(x) -> trunc(x) (round toward zero)
trunc_llvm(x) = Core.Intrinsics.trunc_llvm(x)

# abs_float(x) -> |x|
abs_float(x) = Core.Intrinsics.abs_float(x)

# copysign_float(a, b) -> copy sign of b to a
copysign_float(x, y) = Core.Intrinsics.copysign_float(x, y)

# =============================================================================
# Julia Basic Type Hierarchy
# =============================================================================
# This defines the abstract type hierarchy that mirrors Julia's base types.
# Concrete types (Int64, Float64, etc.) are handled natively by the VM.
#
# This allows users to use standard Julia abstract types in type parameter
# constraints, such as `struct Rational{T<:Integer}`.

# Top type - all types are subtypes of Any
abstract type Any end

# Number hierarchy
abstract type Number <: Any end
abstract type Real <: Number end
abstract type AbstractFloat <: Real end
abstract type Integer <: Real end
abstract type Signed <: Integer end
abstract type Unsigned <: Integer end

# String/Char hierarchy
abstract type AbstractString <: Any end
abstract type AbstractChar <: Any end

# Collection hierarchy (non-parametric for parser compatibility)
abstract type AbstractArray <: Any end
abstract type AbstractVector <: AbstractArray end
abstract type AbstractMatrix <: AbstractArray end
abstract type AbstractDict{K,V} <: Any end
abstract type AbstractSet <: Any end

# Range hierarchy (non-parametric for parser compatibility)
abstract type AbstractRange <: Any end
abstract type AbstractUnitRange <: AbstractRange end

# IO hierarchy (for custom show methods)
abstract type IO <: Any end

# =============================================================================
# Value Types - Types that encode values as type parameters
# =============================================================================
# Val{x} is a singleton type used to pass values as type parameters.
# This enables dispatch on values known at compile time.
#
# Example:
#   f(::Val{1}) = "one"
#   f(::Val{2}) = "two"
#   f(Val(1))  # returns "one"
#   f(Val(2))  # returns "two"

"""
    Val{x}()

A singleton type for encoding value `x` as a type parameter.
Used for compile-time dispatch based on values.

# Examples
```julia
ntuple(i -> i^2, Val{3}())  # (1, 4, 9) - N=3 known at compile time
```
"""
struct Val{x}
end

# Convenience constructor: Val(x) -> Val{x}()
Val(x) = Val{x}()
