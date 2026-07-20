# =============================================================================
# Operators - Pairwise comparison operations
# =============================================================================
# Based on Julia's base/operators.jl

# min: return the smaller of two values
function min(x, y)
    if x < y
        return x
    else
        return y
    end
end

# min: variadic form — reduce pairwise (Issue #2135)
function min(x, y, args...)
    m = min(x, y)
    for a in args
        m = min(m, a)
    end
    return m
end

# max: return the larger of two values
function max(x, y)
    if x > y
        return x
    else
        return y
    end
end

# max: variadic form — reduce pairwise (Issue #2135)
function max(x, y, args...)
    m = max(x, y)
    for a in args
        m = max(m, a)
    end
    return m
end

# Julia Base exposes membership operators as callables in operators.jl.
function ∈(x, itr)
    return in(x, itr)
end

function ∉(x, itr)
    return !in(x, itr)
end

function ∋(itr, x)
    return in(x, itr)
end

function ∌(itr, x)
    return !in(x, itr)
end

# Function composition. Upstream defines `∘` and `ComposedFunction` in
# base/operators.jl. The VM keeps the callable carrier as an internal boundary;
# public composition is a Julia method that calls it.
compose(f, g) = _compose(f, g)
∘(f) = f
∘(f, g) = _compose(f, g)

# minmax: return (min, max) as a tuple
function minmax(x, y)
    if x < y
        return (x, y)
    else
        return (y, x)
    end
end

# copysign: return x with the sign of y
# Based on Julia's base/number.jl:252
function copysign(x, y)
    if signbit(y)
        return -abs(x)
    else
        return abs(x)
    end
end

# Note: flipsign is now defined in number.jl as the generic fallback
# and in int.jl for Int64

# =============================================================================
# Comparison functions
# =============================================================================

# cmp: three-way comparison, returns -1, 0, or 1
function cmp(x, y)
    if x < y
        return -1
    elseif x > y
        return 1
    else
        return 0
    end
end

# isless: strict weak ordering comparison (handles NaN correctly)
# NaN is considered greater than all other values
function isless(x, y)
    # Check if x is NaN
    if x != x
        return false
    end
    # Check if y is NaN
    if y != y
        return true
    end
    return x < y
end

# Float total order (upstream base/float.jl `isless(a::T, b::T) where T<:IEEEFloat`):
# floats form a genuine total order for sorting — NaN sorts greatest and
# `-0.0 < 0.0`. The generic `isless` above returns `false` for `isless(-0.0, 0.0)`
# because `-0.0 < 0.0` is `false`; these same-type specializations restore the
# signed-zero ordering so `sort`/`issorted`/Dict ordering match Julia (Issue #9344).
function _float_isless(a, b)
    a != a && return false # a is NaN: NaN is the largest, never less than anything
    b != b && return true  # b is NaN (a is not): a < NaN
    if a == b
        # `-0.0 == 0.0` is true; distinguish the signed zeros via the sign of 1/x
        # (`1/-0.0 == -Inf`, `1/0.0 == Inf`). Non-zero equal values are not `isless`.
        return (a == 0.0) && ((1.0 / a) < (1.0 / b))
    end
    return a < b
end

isless(a::Float64, b::Float64) = _float_isless(a, b)
isless(a::Float32, b::Float32) = _float_isless(a, b)
isless(a::Float16, b::Float16) = _float_isless(a, b)

# isequal: equality comparison (NaN == NaN is true, unlike ==)
function isequal(x, y)
    # Both NaN
    if x != x && y != y
        return true
    end
    return x == y
end

# Float64 specialization: uses === (bit-identical comparison)
function isequal(a::Float64, b::Float64)
    return a === b
end

function _float_signequal(a, b)
    if a == 0.0 && b == 0.0
        return ((1.0 / a) < 0.0) == ((1.0 / b) < 0.0)
    end
    return (a < 0.0) == (b < 0.0)
end

function _float_isequal(a, b)
    if a != a && b != b
        return true
    end
    return _float_signequal(a, b) && a == b
end

function isequal(a::Float32, b::Float32)
    return a === b
end

function isequal(a::Float16, b::Float16)
    return a === b
end

function isequal(a::Float32, b::Float64)
    return _float_isequal(a, b)
end

function isequal(a::Float64, b::Float32)
    return _float_isequal(a, b)
end

function isequal(a::Float16, b::Float64)
    return _float_isequal(a, b)
end

function isequal(a::Float64, b::Float16)
    return _float_isequal(a, b)
end

function isequal(a::Float16, b::Float32)
    return _float_isequal(a, b)
end

function isequal(a::Float32, b::Float16)
    return _float_isequal(a, b)
end

function isequal(a::Float32, b::Int64)
    return _float_isequal(a, b)
end

function isequal(a::Int64, b::Float32)
    return _float_isequal(a, b)
end

function isequal(a::Float16, b::Int64)
    return _float_isequal(a, b)
end

function isequal(a::Int64, b::Float16)
    return _float_isequal(a, b)
end

# All remaining fixed-width integer × float `isequal` combinations (Issue #8199:
# UInt64/Int128/UInt128/… × Float16/Float32/Float64). Route through the
# signed-zero-aware `_float_isequal`, which compares value-based (`a == b` is now
# exact for every width) AND distinguishes `-0.0` from an integer's `+0`
# (`isequal(0, -0.0)` is false). The concrete `Int64`/`Float*` methods above are
# strictly more specific and still win for those exact pairs; these abstract
# methods catch every other width without enumerating 60 concrete combinations.
# `Integer`/`AbstractFloat` are matched directly (no coercion), so this does NOT
# reintroduce the BigFloat-coercion hazard that blocks concrete numeric `==`
# methods — and `isequal` is never reached from `==`.
function isequal(a::Integer, b::AbstractFloat)
    return _float_isequal(a, b)
end

function isequal(a::AbstractFloat, b::Integer)
    return _float_isequal(a, b)
end

# Julia's generic `isequal(x, y)` delegates to `==`, whose fallback is identity
# equality. Until sjulia's mixed-type `==` fallback reaches full upstream parity
# (Issue #4642), keep String-vs-other comparisons usable for set/unique
# membership tests.
function isequal(a::String, b::String)
    return a == b
end

function isequal(a::String, b)
    return false
end

function isequal(a, b::String)
    return false
end

# Note: Additional isequal specializations (Nothing, Missing, Int64<->Float64,
# Array, Tuple, Expr) are in missing.jl to avoid function index limit in this file.

# isapprox: approximate equality (≈)
# Default tolerance: rtol=sqrt(eps), atol=0
# For scalars: uses abs for comparison
# For arrays: uses L2 norm (requires isa check at runtime)
#
# Upstream Julia's public API takes `atol` / `rtol` as KEYWORD arguments
# (`isapprox(x, y; atol, rtol, ...)`); these keyword methods provide that
# parity. The keyword default for `rtol` follows upstream: it is the
# sqrt-of-eps relative tolerance unless a positive `atol` is given, in which
# case `rtol` defaults to 0 so a pure absolute-tolerance comparison behaves as
# users expect (Issue #5121: unknown keyword arguments are now rejected, so
# `atol=` / `rtol=` must be real keyword parameters here).
function isapprox(x, y; atol=0.0, rtol=(atol > 0.0 ? 0.0 : 1.4901161193847656e-8))
    # Check if inputs are arrays
    if isa(x, Array) && isa(y, Array)
        return _isapprox_array(x, y, rtol, atol)
    else
        return _isapprox_scalar(x, y, rtol, atol)
    end
end

# isapprox with custom tolerances passed positionally (sjulia extension kept for
# backwards compatibility: `isapprox(x, y, rtol, atol)`).
function isapprox(x, y, rtol, atol)
    # Check if inputs are arrays
    if isa(x, Array) && isa(y, Array)
        return _isapprox_array(x, y, rtol, atol)
    else
        return _isapprox_scalar(x, y, rtol, atol)
    end
end

# Internal scalar implementation
function _isapprox_scalar(x, y, rtol, atol)
    return abs(x - y) <= max(atol, rtol * max(abs(x), abs(y)))
end

function _isapprox_scalar(x::AbstractIrrational, y::AbstractIrrational, rtol, atol)
    return _isapprox_scalar_f64(Float64(x), Float64(y), rtol, atol)
end

function _isapprox_scalar(x::AbstractIrrational, y, rtol, atol)
    return _isapprox_scalar_f64(Float64(x), Float64(y), rtol, atol)
end

function _isapprox_scalar(x, y::AbstractIrrational, rtol, atol)
    return _isapprox_scalar_f64(Float64(x), Float64(y), rtol, atol)
end

# isapprox for arrays: uses L2 norm
# This is called from LinearAlgebra module after checking isa(x, Array)
# Supports both real and complex arrays using abs() for magnitude
# Computes element-wise differences to avoid needing array - operator
function _isapprox_array(x, y, rtol, atol)
    # Check lengths match
    n = length(x)
    if n != length(y)
        return false
    end

    # Compute L2 norm of difference element by element
    s = 0.0
    for i in 1:n
        d = x[i] - y[i]  # Element-wise subtraction (scalar - scalar)
        # Use abs()^2 to handle complex values correctly
        ad = abs(d)
        s = s + ad * ad
    end
    diff_norm = sqrt(s)

    # Compute norm of x
    sx = 0.0
    for i in 1:n
        v = x[i]
        av = abs(v)
        sx = sx + av * av
    end
    norm_x = sqrt(sx)

    # Compute norm of y
    sy = 0.0
    for i in 1:n
        v = y[i]
        av = abs(v)
        sy = sy + av * av
    end
    norm_y = sqrt(sy)

    max_norm = max(norm_x, norm_y)
    return diff_norm <= max(atol, rtol * max_norm)
end

# =============================================================================
# Identity operators
# =============================================================================

# !== (≢): not identical (negation of ===)
# Based on Julia's base/operators.jl
# !==(a, b) is equivalent to !(a === b)
function !==(a, b)
    return !(a === b)
end

# =============================================================================
# Type equality
# =============================================================================
# Based on Julia's base/operators.jl:295-297
# In Julia, Type equality uses ccall(:jl_types_equal). SubsetJuliaVM routes this
# through a VM intrinsic so UnionAll aliases compare semantically without making
# `===` accept aliases that Julia keeps distinct.

==(T::Type, S::Type) = _type_equal(T, S)
!=(T::Type, S::Type) = !_type_equal(T, S)

==(f::Function, g::Function) = f === g

# Unicode aliases for identity operators (≡ and ≢)
# Based on Julia's base/operators.jl:348,370
# These operators are handled directly in the lowering:
#   ≡ is lowered to === (object identity)
#   ≢ is lowered to !== (not identical)
# Export declarations are in exports.jl

# =============================================================================
# Type widening
# =============================================================================
# Based on Julia's base/operators.jl and base/int.jl

# widen: return a type one step wider than the argument
#
# Matches upstream exactly. The type-based rules live across several upstream
# files (base/int.jl, base/float.jl, base/gmp.jl, base/mpfr.jl); sjulia keeps
# them centralized here for cohesion (Issue #5110).
#
#   widen(::Type{Int8})    === Int16   (base/int.jl:871)
#   widen(::Type{Int16})   === Int32
#   widen(::Type{Int32})   === Int64
#   widen(::Type{Int64})   === Int128
#   widen(::Type{Int128})  === BigInt  (base/gmp.jl:280)
#   widen(::Type{UInt8})   === UInt16
#   ...
#   widen(::Type{UInt128}) === BigInt  (base/gmp.jl:281)
#   widen(::Type{BigInt})  === BigInt
#   widen(::Type{Float16}) === Float32 (base/float.jl:488)
#   widen(::Type{Float32}) === Float64
#   widen(::Type{Float64}) === BigFloat(base/mpfr.jl:300)
#   widen(::Type{BigFloat})=== BigFloat

# Signed integers (base/int.jl)
widen(::Type{Int8}) = Int16
widen(::Type{Int16}) = Int32
widen(::Type{Int32}) = Int64
widen(::Type{Int64}) = Int128
# Int128 -> BigInt (base/gmp.jl)
widen(::Type{Int128}) = BigInt

# Unsigned integers (base/int.jl)
widen(::Type{UInt8}) = UInt16
widen(::Type{UInt16}) = UInt32
widen(::Type{UInt32}) = UInt64
widen(::Type{UInt64}) = UInt128
# UInt128 -> BigInt (base/gmp.jl)
widen(::Type{UInt128}) = BigInt

# BigInt is already arbitrary precision (base/gmp.jl)
widen(::Type{BigInt}) = BigInt

# Floating point (base/float.jl, base/mpfr.jl)
widen(::Type{Float16}) = Float32
widen(::Type{Float32}) = Float64
widen(::Type{Float64}) = BigFloat
# BigFloat is already arbitrary precision (base/mpfr.jl)
widen(::Type{BigFloat}) = BigFloat

# Generic value-based widen: convert the value to its widened type
# (base/operators.jl:954) -- widen(x::T) where {T} = convert(widen(T), x)
widen(x::T) where {T} = convert(widen(T), x)

# =============================================================================
# Identity function
# =============================================================================
# Based on Julia's base/operators.jl:584

# identity: return the argument unchanged
# This is useful as a "do nothing" function argument, or as a default function parameter.
identity(x) = x

# =============================================================================
# Pipe operator
# =============================================================================
# Based on Julia's base/operators.jl:980

# |>: infix operator which applies function f to argument x
# This allows f(g(x)) to be written as x |> g |> f
|>(x, f) = f(x)

# =============================================================================
# isunordered - check if value is unordered (NaN, Missing)
# =============================================================================
# Based on Julia's base/operators.jl:291-293
# Returns true for values where comparisons are undefined.
# NaN and missing are unordered (comparisons with them don't follow total order).
# Note: Missing specializations are in missing.jl (isunordered(::Missing) = true)

isunordered(x) = false
isunordered(x::Float64) = isnan(x)

# =============================================================================
# isgreater - Descending total order comparison
# =============================================================================
# Based on Julia's base/operators.jl:277
#
# isgreater(x, y) tests whether x is greater than y according to a fixed total
# order compatible with min. NaN and missing are ordered as smallest values.
# This is NOT the inverse of isless.

function isgreater(x, y)
    if isunordered(x) || isunordered(y)
        return isless(x, y)
    else
        return isless(y, x)
    end
end

# =============================================================================
# Modular arithmetic - mod1, fld1, fldmod1
# =============================================================================
# Based on Julia's base/operators.jl:893-930

# mod1: modulus after flooring division, returning a value in (0, y]
# Unlike mod(x, y) which returns values in [0, y), mod1(x, y) returns values in (0, y]
# mod1(4, 2) = 2 (not 0)
# mod1(3, 3) = 3 (not 0)
function mod1(x::Int64, y::Int64)
    m = mod(x, y)
    if m == 0
        return y
    else
        return m
    end
end

function mod1(x::Float64, y::Float64)
    m = mod(x, y)
    if m == 0.0
        return y
    else
        return m
    end
end

# fld1: flooring division, returning a value consistent with mod1(x, y)
# Based on Julia's base/operators.jl:917-921
# The relationship: x == (fld1(x, y) - 1) * y + mod1(x, y)
function fld1(x::Int64, y::Int64)
    # Use Float64 version for simplicity
    m = mod1(x, y)
    return fld((x - m) + y, y)
end

function fld1(x::Float64, y::Float64)
    m = mod1(x, y)
    return fld((x - m) + y, y)
end

# fldmod1: return both fld1 and mod1 as a tuple
# Based on Julia's base/operators.jl:930
fldmod1(x, y) = (fld1(x, y), mod1(x, y))

# =============================================================================
# Returns - functor that returns a constant value
# =============================================================================
# Based on Julia's base/operators.jl
#
# Returns(x) creates a callable that always returns x
# Useful for HOFs: filter(Returns(true), arr) keeps all elements

struct Returns
    value
end

# Make Returns callable - always returns the stored value
# Note: This requires special handling in the VM for callable structs
# For now, we define a helper function
function call_returns(r::Returns)
    return r.value
end

# =============================================================================
# Fix1 / Fix2 - partially-applied functions
# =============================================================================
# Based on Julia's base/operators.jl (Fix1/Fix2; upstream also has Fix{N}).
#
# Upstream 1.12 defines `Fix{N,F,T} <: Function` with `Fix1 = Fix{1,F,T}` and
# `Fix2 = Fix{2,F,T}` aliases, dispatching on the integer parameter `N`.
# SubsetJuliaVM does not yet support integer type-parameter dispatch on a
# partially-applied inner constructor `Fix{1}(f, x)` (see Issue #5127 notes), so
# `Fix1`/`Fix2` are provided here as the two concrete partial-application types
# the standard library actually uses. The runtime behavior matches upstream:
#
#   Fix1(f, x)(y) == f(x, y)   # fix the first argument
#   Fix2(f, x)(y) == f(y, x)   # fix the second argument
#
# Upstream uses `Fix2` to represent the curried operator forms (`==(x)`,
# `>(x)`, `in(c)`, ...). See the note after the type definitions for how those
# forms behave in SubsetJuliaVM today.

"""
    Fix1(f, x)

A type representing a partially-applied version of the two-argument function
`f`, with the first argument fixed to the value `x`. In other words,
`Fix1(f, x)` behaves similarly to `y -> f(x, y)`.
"""
struct Fix1{F,T} <: Function
    f::F
    x::T
end

(f::Fix1)(y) = f.f(f.x, y)

"""
    Fix2(f, x)

A type representing a partially-applied version of the two-argument function
`f`, with the second argument fixed to the value `x`. In other words,
`Fix2(f, x)` behaves similarly to `y -> f(y, x)`.

The curried comparison/membership operators (`==(x)`, `>(x)`, `in(c)`, ...)
return a `Fix2`.
"""
struct Fix2{F,T} <: Function
    f::F
    x::T
end

(f::Fix2)(y) = f.f(y, f.x)

# Curried operators
# -----------------
# Upstream also defines single-argument operator forms that return a `Fix2`,
# e.g. `==(x) = Fix2(==, x)` so `==(2)` is `y -> y == 2`. SubsetJuliaVM already
# rewrites the comparison-operator partial forms (`==(x)`, `!=(x)`, `>(x)`,
# `<(x)`, `>=(x)`, `<=(x)`, `===(x)`, `!==(x)`) into closures during lowering
# (Issue #3119), so those forms work as values today. They produce an anonymous
# closure rather than a `Base.Fix2{...}` instance; converting the lowering to
# emit `Fix2` is tracked separately (Issue #5127). Constructing `Fix2`/`Fix1`
# directly (`Fix2(==, 2)`, `Base.Fix2(^, 2)`, `Fix1(-, 10)`) yields the proper
# partial-application type.

# `isequal` is an ordinary function (not a comparison operator), so the operator
# partial-application lowering (Issue #3119) does not rewrite its single-argument
# form. Define it to return the curried predicate so `map(isequal(3), v)` /
# `filter(isequal(2), v)` / `findfirst(isequal(3), v)` work. Upstream returns a
# `Base.Fix2`, but — exactly like the curried comparison operators (`==(x)`),
# which sjulia represents as anonymous closures rather than `Base.Fix2` instances
# (Issue #5127) — this returns a closure so it flows through every HOF
# consistently (the HOF/inference paths handle closures, not bare `Fix2` values).
# NOTE: upstream defines NO single-argument `isless(x)` form, so `isless(2)`
# correctly stays a MethodError (Issue #5662).
isequal(x) = y -> isequal(y, x)
