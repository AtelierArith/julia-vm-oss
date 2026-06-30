# Type promotion system for SubsetJuliaVM
# Implements promote_rule, promote_type, and promote functions

# =============================================================================
# Default fallback
# =============================================================================

# Default fallback - returns Union{} (Bottom) for undefined type pairs
function promote_rule(::Type{T}, ::Type{S}) where {T, S}
    Union{}
end

# Same type - return that type
function promote_type(::Type{T}, ::Type{T}) where {T}
    T
end

# =============================================================================
# Integer promotion rules (mirrors julia/base/int.jl:775-788 exactly)
# =============================================================================
# Wider signed/unsigned integers promote narrower integers (signed OR unsigned),
# and at equal width unsigned wins over signed. These use Union-typed Type{}
# arguments exactly as upstream so the full integer lattice is covered with a
# minimal, maintainable set of methods (Issue #5070).
#
#   promote_rule(::Type{Int16}, ::Union{Type{Int8}, Type{UInt8}}) = Int16
#   ...
# `promote_type` tries both argument orders via promote_result, so registering
# each rule once (wider type first) makes promotion symmetric and order-free.

promote_rule(::Type{Int16}, ::Type{Int8}) = Int16
promote_rule(::Type{Int16}, ::Type{UInt8}) = Int16

promote_rule(::Type{Int32}, ::Type{Int16}) = Int32
promote_rule(::Type{Int32}, ::Type{Int8}) = Int32
promote_rule(::Type{Int32}, ::Type{UInt16}) = Int32
promote_rule(::Type{Int32}, ::Type{UInt8}) = Int32

promote_rule(::Type{Int64}, ::Type{Int16}) = Int64
promote_rule(::Type{Int64}, ::Type{Int32}) = Int64
promote_rule(::Type{Int64}, ::Type{Int8}) = Int64
promote_rule(::Type{Int64}, ::Type{UInt16}) = Int64
promote_rule(::Type{Int64}, ::Type{UInt32}) = Int64
promote_rule(::Type{Int64}, ::Type{UInt8}) = Int64

promote_rule(::Type{Int128}, ::Type{Int16}) = Int128
promote_rule(::Type{Int128}, ::Type{Int32}) = Int128
promote_rule(::Type{Int128}, ::Type{Int64}) = Int128
promote_rule(::Type{Int128}, ::Type{Int8}) = Int128
promote_rule(::Type{Int128}, ::Type{UInt16}) = Int128
promote_rule(::Type{Int128}, ::Type{UInt32}) = Int128
promote_rule(::Type{Int128}, ::Type{UInt64}) = Int128
promote_rule(::Type{Int128}, ::Type{UInt8}) = Int128

promote_rule(::Type{UInt16}, ::Type{Int8}) = UInt16
promote_rule(::Type{UInt16}, ::Type{UInt8}) = UInt16

promote_rule(::Type{UInt32}, ::Type{Int16}) = UInt32
promote_rule(::Type{UInt32}, ::Type{Int8}) = UInt32
promote_rule(::Type{UInt32}, ::Type{UInt16}) = UInt32
promote_rule(::Type{UInt32}, ::Type{UInt8}) = UInt32

promote_rule(::Type{UInt64}, ::Type{Int16}) = UInt64
promote_rule(::Type{UInt64}, ::Type{Int32}) = UInt64
promote_rule(::Type{UInt64}, ::Type{Int8}) = UInt64
promote_rule(::Type{UInt64}, ::Type{UInt16}) = UInt64
promote_rule(::Type{UInt64}, ::Type{UInt32}) = UInt64
promote_rule(::Type{UInt64}, ::Type{UInt8}) = UInt64

promote_rule(::Type{UInt128}, ::Type{Int16}) = UInt128
promote_rule(::Type{UInt128}, ::Type{Int32}) = UInt128
promote_rule(::Type{UInt128}, ::Type{Int64}) = UInt128
promote_rule(::Type{UInt128}, ::Type{Int8}) = UInt128
promote_rule(::Type{UInt128}, ::Type{UInt16}) = UInt128
promote_rule(::Type{UInt128}, ::Type{UInt32}) = UInt128
promote_rule(::Type{UInt128}, ::Type{UInt64}) = UInt128
promote_rule(::Type{UInt128}, ::Type{UInt8}) = UInt128

# Same-width signed/unsigned: unsigned wins (julia/base/int.jl:784-788)
function promote_rule(::Type{UInt8}, ::Type{Int8})
    UInt8
end

function promote_rule(::Type{UInt16}, ::Type{Int16})
    UInt16
end

function promote_rule(::Type{UInt32}, ::Type{Int32})
    UInt32
end

function promote_rule(::Type{UInt64}, ::Type{Int64})
    UInt64
end

function promote_rule(::Type{UInt128}, ::Type{Int128})
    UInt128
end

# Bool promotes to any other Number (julia/base/bool.jl:6)
#   promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number} = T
function promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number}
    T
end

# =============================================================================
# Float promotion rules
# =============================================================================

# Integers promote to Float64
function promote_rule(::Type{Float64}, ::Type{Int64})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{Int128})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{Int32})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{Int16})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{Int8})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{Bool})
    Float64
end

# Unsigned integers promote to Float64
function promote_rule(::Type{Float64}, ::Type{UInt128})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{UInt64})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{UInt32})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{UInt16})
    Float64
end

function promote_rule(::Type{Float64}, ::Type{UInt8})
    Float64
end

# Float32 promotes to Float64
function promote_rule(::Type{Float64}, ::Type{Float32})
    Float64
end

# Integers promote to Float32
function promote_rule(::Type{Float32}, ::Type{Int64})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{Int32})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{Int16})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{Int8})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{Int128})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{Bool})
    Float32
end

# Unsigned integers promote to Float32
function promote_rule(::Type{Float32}, ::Type{UInt128})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{UInt64})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{UInt32})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{UInt16})
    Float32
end

function promote_rule(::Type{Float32}, ::Type{UInt8})
    Float32
end

# =============================================================================
# Float16 promotion rules (from julia/base/float.jl)
# =============================================================================

# Float16 promotes to Float32 and Float64
function promote_rule(::Type{Float32}, ::Type{Float16})
    Float32
end

function promote_rule(::Type{Float64}, ::Type{Float16})
    Float64
end

# Integers promote to Float16
function promote_rule(::Type{Float16}, ::Type{Int64})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{Int32})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{Int16})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{Int8})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{Int128})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{Bool})
    Float16
end

# Unsigned integers promote to Float16
function promote_rule(::Type{Float16}, ::Type{UInt128})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{UInt64})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{UInt32})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{UInt16})
    Float16
end

function promote_rule(::Type{Float16}, ::Type{UInt8})
    Float16
end

# =============================================================================
# Complex promotion rules (mirrors julia/base/complex.jl:49-52 exactly)
# =============================================================================
# A single pair of parametric rules covers the entire Complex lattice:
#   promote_rule(::Type{Complex{T}}, ::Type{S})           where {T<:Real,S<:Real} = Complex{promote_type(T,S)}
#   promote_rule(::Type{Complex{T}}, ::Type{Complex{S}})  where {T<:Real,S<:Real} = Complex{promote_type(T,S)}
# This makes Complex promotion symmetric and order-free (Issue #5070), and it
# recurses through promote_type on the element types so any Real element pair
# supported by promote_type is handled (e.g. Complex{Int8} + Float64 ->
# Complex{Float64}).

function promote_rule(::Type{Complex{T}}, ::Type{S}) where {T<:Real, S<:Real}
    Complex{promote_type(T, S)}
end

function promote_rule(::Type{Complex{T}}, ::Type{Complex{S}}) where {T<:Real, S<:Real}
    Complex{promote_type(T, S)}
end

# =============================================================================
# Rational promotion rules (semantics from julia/base/rational.jl:221-223)
# =============================================================================
# Upstream uses three parametric rules:
#   promote_rule(::Type{Rational{T}}, ::Type{S})           where {T<:Integer,S<:Integer}       = Rational{promote_type(T,S)}
#   promote_rule(::Type{Rational{T}}, ::Type{Rational{S}})  where {T<:Integer,S<:Integer}       = Rational{promote_type(T,S)}
#   promote_rule(::Type{Rational{T}}, ::Type{S})           where {T<:Integer,S<:AbstractFloat} = promote_type(T,S)
#
# The Rational+Rational form dispatches reliably through a variable in the VM
# (distinguished by the Rational{} constructor on the second arg), so it stays
# parametric. The Rational+Integer and Rational+AbstractFloat forms share the
# signature shape (Type{Rational{T}}, Type{S}) and the VM cannot tell their
# S-bounds apart when promote_type passes the types through variables, so they
# are enumerated with concrete first args and Union-of-concrete second args
# (Issue #5070). This makes promotion complete (UInt / Int128 element partners
# now covered) and fixes the prior bug where Rational{BigInt}+Float promoted to
# the narrow Float instead of BigFloat.

# Rational{T} + Rational{S}  =>  Rational{promote_type(T, S)} (parametric; works via variable)
function promote_rule(::Type{Rational{T}}, ::Type{Rational{S}}) where {T<:Integer, S<:Integer}
    Rational{promote_type(T, S)}
end

# Rational{R} + Integer  =>  Rational{promote_type(R, S)}
# (concrete first arg + Union of concrete integer types; the VM cannot
#  dispatch on an abstract S<:Integer bound through a variable, Issue #5070)
function promote_rule(::Type{Rational{Int8}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Int128})
    Rational{Int128}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Int16})
    Rational{Int16}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Int32})
    Rational{Int32}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Int64})
    Rational{Int64}
end
function promote_rule(::Type{Rational{Int8}}, ::Union{Type{Bool}, Type{Int8}})
    Rational{Int8}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{UInt128})
    Rational{UInt128}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{UInt16})
    Rational{UInt16}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{UInt32})
    Rational{UInt32}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{UInt64})
    Rational{UInt64}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{UInt8})
    Rational{UInt8}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Int128})
    Rational{Int128}
end
function promote_rule(::Type{Rational{Int16}}, ::Union{Type{Bool}, Type{Int16}, Type{Int8}, Type{UInt8}})
    Rational{Int16}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Int32})
    Rational{Int32}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Int64})
    Rational{Int64}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{UInt128})
    Rational{UInt128}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{UInt16})
    Rational{UInt16}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{UInt32})
    Rational{UInt32}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{UInt64})
    Rational{UInt64}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{Int128})
    Rational{Int128}
end
function promote_rule(::Type{Rational{Int32}}, ::Union{Type{Bool}, Type{Int16}, Type{Int32}, Type{Int8}, Type{UInt16}, Type{UInt8}})
    Rational{Int32}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{Int64})
    Rational{Int64}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{UInt128})
    Rational{UInt128}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{UInt32})
    Rational{UInt32}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{UInt64})
    Rational{UInt64}
end
function promote_rule(::Type{Rational{Int64}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int64}}, ::Type{Int128})
    Rational{Int128}
end
function promote_rule(::Type{Rational{Int64}}, ::Type{Int64})
    Rational{Int64}
end
function promote_rule(::Type{Rational{Int64}}, ::Union{Type{Bool}, Type{Int16}, Type{Int32}, Type{Int64}, Type{Int8}, Type{UInt16}, Type{UInt32}, Type{UInt8}})
    Rational{Int64}
end
function promote_rule(::Type{Rational{Int64}}, ::Type{UInt128})
    Rational{UInt128}
end
function promote_rule(::Type{Rational{Int64}}, ::Type{UInt64})
    Rational{UInt64}
end
function promote_rule(::Type{Rational{BigInt}}, ::Union{Type{BigInt}, Type{Bool}, Type{Int128}, Type{Int16}, Type{Int32}, Type{Int64}, Type{Int8}, Type{UInt128}, Type{UInt16}, Type{UInt32}, Type{UInt64}, Type{UInt8}})
    Rational{BigInt}
end

# Rational{R} + AbstractFloat  =>  promote_type(R, S)  (float wins; for
# Rational{BigInt} this is BigFloat because promote_type(BigInt, Float) === BigFloat)
function promote_rule(::Type{Rational{Int8}}, ::Type{BigFloat})
    BigFloat
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Float16})
    Float16
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Float32})
    Float32
end
function promote_rule(::Type{Rational{Int8}}, ::Type{Float64})
    Float64
end
function promote_rule(::Type{Rational{Int16}}, ::Type{BigFloat})
    BigFloat
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Float16})
    Float16
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Float32})
    Float32
end
function promote_rule(::Type{Rational{Int16}}, ::Type{Float64})
    Float64
end
function promote_rule(::Type{Rational{Int32}}, ::Type{BigFloat})
    BigFloat
end
function promote_rule(::Type{Rational{Int32}}, ::Type{Float16})
    Float16
end
function promote_rule(::Type{Rational{Int32}}, ::Type{Float32})
    Float32
end
function promote_rule(::Type{Rational{Int32}}, ::Type{Float64})
    Float64
end
function promote_rule(::Type{Rational{Int64}}, ::Type{BigFloat})
    BigFloat
end
function promote_rule(::Type{Rational{Int64}}, ::Type{Float16})
    Float16
end
function promote_rule(::Type{Rational{Int64}}, ::Type{Float32})
    Float32
end
function promote_rule(::Type{Rational{Int64}}, ::Type{Float64})
    Float64
end
function promote_rule(::Type{Rational{BigInt}}, ::Union{Type{BigFloat}, Type{Float16}, Type{Float32}, Type{Float64}})
    BigFloat
end

# =============================================================================
# BigInt / BigFloat promotion rules (semantics from julia/base/gmp.jl:479,
# mpfr.jl:558-560)
# =============================================================================
# BigInt + Integer  =>  BigInt ;  BigInt + AbstractFloat  =>  BigFloat
# (Union of concrete partners; abstract Type{<:Integer} bound does not
#  dispatch through a variable in the VM, Issue #5070)
promote_rule(::Type{BigInt}, ::Type{Bool}) = BigInt
promote_rule(::Type{BigInt}, ::Type{Int8}) = BigInt
promote_rule(::Type{BigInt}, ::Type{Int16}) = BigInt
promote_rule(::Type{BigInt}, ::Type{Int32}) = BigInt
promote_rule(::Type{BigInt}, ::Type{Int64}) = BigInt
promote_rule(::Type{BigInt}, ::Type{Int128}) = BigInt
promote_rule(::Type{BigInt}, ::Type{UInt8}) = BigInt
promote_rule(::Type{BigInt}, ::Type{UInt16}) = BigInt
promote_rule(::Type{BigInt}, ::Type{UInt32}) = BigInt
promote_rule(::Type{BigInt}, ::Type{UInt64}) = BigInt
promote_rule(::Type{BigInt}, ::Type{UInt128}) = BigInt
function promote_rule(::Type{BigInt}, ::Union{Type{Float16}, Type{Float32}, Type{Float64}})
    BigFloat
end

# BigFloat + Real  =>  BigFloat
function promote_rule(::Type{BigFloat}, ::Union{Type{Bool}, Type{Int8}, Type{Int16}, Type{Int32}, Type{Int64}, Type{Int128}, Type{UInt8}, Type{UInt16}, Type{UInt32}, Type{UInt64}, Type{UInt128}, Type{Float16}, Type{Float32}, Type{Float64}, Type{BigInt}})
    BigFloat
end

# =============================================================================
# promote_type - find common type using promote_rule
# =============================================================================

# Try promote_rule in both orders (symmetric/order-independent), then fall back
# to typejoin when neither direction defines a rule. This mirrors upstream's
#   promote_type(::Type{T}, ::Type{S}) where {T,S} =
#       promote_result(T, S, promote_rule(T,S), promote_rule(S,T))
#   promote_result(::Type,::Type,::Type{R},::Type) = R           # rule found
#   promote_result(::Type{T},::Type{S},::Type{Bottom},::Type{Bottom}) = typejoin(T, S)
# so that user-defined promote_rule methods extend promotion automatically and
# promote_type(T,S) === promote_type(S,T) for every pair (Issue #5070).
promote_type() = Union{}

function promote_type(::Type{T}, ::Type{S}) where {T, S}
    R1 = promote_rule(T, S)
    R2 = promote_rule(S, T)
    # Check both directions
    if R1 !== Union{}
        return R1
    elseif R2 !== Union{}
        return R2
    else
        # No rule in either direction: fall back to the common supertype,
        # matching Julia's typejoin fallback (e.g. promote_type(Int, String)
        # === Any, promote_type(Int8, Float64-less pairs) === their join).
        return typejoin(T, S)
    end
end

# 3-argument version: promote_type(T1, T2, T3) = promote_type(promote_type(T1, T2), T3)
function promote_type(::Type{T1}, ::Type{T2}, ::Type{T3}) where {T1, T2, T3}
    promote_type(promote_type(T1, T2), T3)
end

# 4-argument version
function promote_type(::Type{T1}, ::Type{T2}, ::Type{T3}, ::Type{T4}) where {T1, T2, T3, T4}
    promote_type(promote_type(T1, T2), T3, T4)
end

# =============================================================================
# promote_typejoin - Union-aware typejoin (Issue #5113)
# =============================================================================
#
# Mirrors `julia/base/promotion.jl`. `promote_typejoin(a, b)` computes a type
# containing both `a` and `b`: it falls back to `typejoin`, but PRESERVES a
# `Nothing`/`Missing` component as a small `Union` instead of widening it away.
# Used by container element-type inference to avoid over-widening (e.g. so
# `[1, nothing]` can infer `Union{Nothing, Int}` rather than `Any`).
#
#   Base.promote_typejoin(Int, Float64)  === Real
#   Base.promote_typejoin(Int, Nothing)  === Union{Nothing, Int64}
#   Base.promote_typejoin(Int, Missing)  === Union{Missing, Int64}

# Return an upper bound on type `a` with type `b` removed, such that
# `result <: a` and `Union{result, b} == Union{a, b}`. `b` is always a simple
# (non-broken-subtype) type here. SubsetJuliaVM cannot field-access a `Union`'s
# components from pure Julia, so the upstream recursive `Union` branch is
# omitted; the callers below only ever pass a non-`Union` `a` (a single type or
# `Nothing`/`Missing`), for which `a <: b ? Union{} : a` is exact.
typesplit(a, b) = a <: b ? Union{} : a

# Subtract a `Nothing`/`Missing` component (if present) from `a`, leaving an
# upper bound that no longer contains it. Mirrors upstream `_promote_typesubtract`.
function _promote_typesubtract(a)
    a === Any && return a
    a >: Union{Nothing, Missing} && return typesplit(a, Union{Nothing, Missing})
    a >: Nothing && return typesplit(a, Nothing)
    a >: Missing && return typesplit(a, Missing)
    return a
end

function promote_typejoin(a, b)
    c = typejoin(_promote_typesubtract(a), _promote_typesubtract(b))
    return Union{a, b, c}
end

# =============================================================================
# promote_op - operation result type (Issue #5114)
# =============================================================================
#
# Mirrors `julia/base/promotion.jl`. `promote_op(f, Ts...)` returns an upper
# bound on the type of `f(xs...)` where `xs::Ts`, inferred from the types alone
# (no values). Implemented, as upstream, via type inference on the tuple
# signature `Tuple{Ts...}` (`Core.Compiler.return_type` / `infer_return_type`).
#
#   Base.promote_op(+, Int, Float64) === Float64
#   Base.promote_op(*, Int, Int)     === Int64
#   Base.promote_op(/, Int, Int)     === Float64
#
# NOTE: the result is only an upper bound; for functions whose return type the
# subset's inference cannot pin down it may widen to `Any`.
#
# Upstream's signature is `promote_op(f, S::Type...)`. The `::Type...` vararg
# annotation is dropped by the base-prelude lowering, so the bare `S...` vararg
# is used here; callers always pass type objects, matching the upstream contract.
function promote_op(f, S...)
    argT = Tuple{S...}
    return infer_return_type(f, argT)
end

# =============================================================================
# promote - convert values to common type
# =============================================================================

# Same-type fast path: no conversion needed when both args have the same type.
# This prevents unnecessary promote_type/convert calls and, crucially,
# avoids infinite recursion when Number fallback operators call promote
# on already-promoted (same-type) values.
function promote(x::T, y::T) where {T}
    (x, y)
end

# Mirrors Julia's `base/gmp.jl` rule:
#   promote_rule(::Type{BigInt}, ::Type{<:Integer}) = BigInt
# sjulia does not yet dispatch `Type{<:Integer}` precisely enough in the generic
# promote(x, y) body, so route the value-level BigInt/Integer case explicitly.
function promote(x::BigInt, y::Integer)
    (x, BigInt(y))
end

function promote(x::Integer, y::BigInt)
    (BigInt(x), y)
end

function promote(x::BigInt, y::Rational)
    (Rational{BigInt}(x, big(1)), convert(Rational{BigInt}, y))
end

function promote(x::Rational, y::BigInt)
    (convert(Rational{BigInt}, x), Rational{BigInt}(y, big(1)))
end

function promote(x::AbstractIrrational, y::Float64)
    (Float64(x), y)
end

function promote(x::Float64, y::AbstractIrrational)
    (x, Float64(y))
end

function promote(x::AbstractIrrational, y::Float32)
    (Float32(x), y)
end

function promote(x::Float32, y::AbstractIrrational)
    (x, Float32(y))
end

function promote(x::AbstractIrrational, y::BigFloat)
    (BigFloat(x), y)
end

function promote(x::BigFloat, y::AbstractIrrational)
    (x, BigFloat(y))
end

function promote(x::AbstractIrrational, y::Integer)
    (Float64(x), Float64(y))
end

function promote(x::Integer, y::AbstractIrrational)
    (Float64(x), Float64(y))
end

function promote(x, y)
    target_type = promote_type(typeof(x), typeof(y))
    # Use intermediate variables to work around tuple construction bug
    # when function call results are directly used as tuple elements
    cx = convert(target_type, x)
    cy = convert(target_type, y)
    (cx, cy)
end

# 3-argument version: convert all three to common type
function promote(x, y, z)
    target_type = promote_type(typeof(x), typeof(y), typeof(z))
    # Use intermediate variables to work around tuple construction bug
    cx = convert(target_type, x)
    cy = convert(target_type, y)
    cz = convert(target_type, z)
    (cx, cy, cz)
end

# =============================================================================
# convert implementations
# =============================================================================

# Identity conversion
function convert(::Type{T}, x::T) where {T}
    x
end

function convert(::Type{Union{Nothing, Int64}}, x::Int64)
    x
end

function convert(::Type{Union{Nothing, Int64}}, x::Nothing)
    nothing
end

# Number conversion fallback
# Based on Julia's base/number.jl:
#   convert(::Type{T}, x::Number) where {T<:Number} = T(x)::T
function convert(::Type{T}, x::Number) where {T<:Number}
    T(x)
end

# Integer conversions
function convert(::Type{Int64}, x::Bool)
    x ? Int64(1) : Int64(0)
end

function convert(::Type{Int64}, x::Int32)
    Int64(x)
end

function convert(::Type{Int64}, x::Int16)
    Int64(x)
end

function convert(::Type{Int64}, x::Int8)
    Int64(x)
end

function convert(::Type{Int32}, x::Bool)
    x ? Int32(1) : Int32(0)
end

function convert(::Type{Int32}, x::Int16)
    Int32(x)
end

function convert(::Type{Int32}, x::Int8)
    Int32(x)
end

# Float conversions
function convert(::Type{Float64}, x::Int64)
    Float64(x)
end

function convert(::Type{Float64}, x::Int32)
    Float64(x)
end

function convert(::Type{Float64}, x::Int16)
    Float64(x)
end

function convert(::Type{Float64}, x::Int8)
    Float64(x)
end

function convert(::Type{Float64}, x::Bool)
    x ? 1.0 : 0.0
end

function convert(::Type{Float64}, x::Float32)
    Float64(x)
end

function convert(::Type{Float32}, x::Int64)
    Float32(x)
end

function convert(::Type{Float32}, x::Bool)
    x ? Float32(1.0) : Float32(0.0)
end

function convert(::Type{Int64}, x::Float64)
    # Mirror upstream `convert(::Type{T}, x::Number) where {T<:Number} = T(x)::T`
    # (base/number.jl): route through the Int64 constructor so non-integral
    # floats throw InexactError instead of being silently truncated. A `floor`
    # here would make convert(Int64, 2.5) == 2, diverging from Julia (Issue #5496).
    Int64(x)
end

# Complex conversions - explicit types
function convert(::Type{Complex{Float64}}, x::Float64)
    Complex{Float64}(x, 0.0)
end

function convert(::Type{Complex{Float64}}, x::Int64)
    Complex{Float64}(Float64(x), 0.0)
end

function convert(::Type{Complex{Float64}}, x::Bool)
    Complex{Float64}(Float64(x), 0.0)
end

function convert(::Type{Complex{Float64}}, x::Float32)
    Complex{Float64}(Float64(x), 0.0)
end

function convert(::Type{Complex{Int64}}, x::Int64)
    Complex{Int64}(x, Int64(0))
end

function convert(::Type{Complex{Int64}}, x::Bool)
    Complex{Int64}(Int64(x), Int64(0))
end

function convert(::Type{Complex{Float32}}, x::Float32)
    Complex{Float32}(x, Float32(0.0))
end

function convert(::Type{Complex{Float32}}, x::Int64)
    Complex{Float32}(Float32(x), Float32(0.0))
end

function convert(::Type{Complex{Float32}}, x::Bool)
    Complex{Float32}(Float32(x), Float32(0.0))
end

function convert(::Type{Complex{Float32}}, x::Float64)
    Complex{Float32}(Float32(x), Float32(0.0))
end

function convert(::Type{Complex{Float64}}, z::Complex{Float64})
    z
end

function convert(::Type{Complex{Float64}}, z::Complex{Int64})
    Complex{Float64}(Float64(z.re), Float64(z.im))
end

function convert(::Type{Complex{Float64}}, z::Complex{Bool})
    Complex{Float64}(Float64(z.re), Float64(z.im))
end

function convert(::Type{Complex{Float64}}, z::Complex{Float32})
    Complex{Float64}(Float64(z.re), Float64(z.im))
end

function convert(::Type{Complex{Int64}}, z::Complex{Int64})
    z
end

function convert(::Type{Complex{Int64}}, z::Complex{Bool})
    Complex{Int64}(Int64(z.re), Int64(z.im))
end

function convert(::Type{Complex{Float32}}, z::Complex{Float32})
    z
end

function convert(::Type{Complex{Float32}}, z::Complex{Int64})
    Complex{Float32}(Float32(z.re), Float32(z.im))
end

function convert(::Type{Complex{Float32}}, z::Complex{Bool})
    Complex{Float32}(Float32(z.re), Float32(z.im))
end

# =============================================================================
# Rational conversions (explicit for each Integer subtype)
# =============================================================================
# Based on Julia's base/rational.jl convert methods
# Note: Generic where {T<:Integer} patterns would cause dispatch ambiguity
# in the VM, so we use explicit methods for each supported Integer type.

# Identity: Rational{T} → Rational{T}
function convert(::Type{Rational{Int64}}, x::Rational{Int64})
    x
end

function convert(::Type{Rational{Int32}}, x::Rational{Int32})
    x
end

function convert(::Type{Rational{Int16}}, x::Rational{Int16})
    x
end

function convert(::Type{Rational{Int8}}, x::Rational{Int8})
    x
end

# Cross-type Rational conversions
function convert(::Type{Rational{Int64}}, x::Rational)
    Rational{Int64}(Int64(x.num), Int64(x.den))
end

function convert(::Type{Rational{Int32}}, x::Rational)
    Rational{Int32}(Int32(x.num), Int32(x.den))
end

function convert(::Type{Rational{Int16}}, x::Rational)
    Rational{Int16}(Int16(x.num), Int16(x.den))
end

function convert(::Type{Rational{Int8}}, x::Rational)
    Rational{Int8}(Int8(x.num), Int8(x.den))
end

# Integer → Rational{Int64}
function convert(::Type{Rational{Int64}}, x::Int64)
    Rational{Int64}(x, Int64(1))
end

function convert(::Type{Rational{Int64}}, x::Int32)
    Rational{Int64}(Int64(x), Int64(1))
end

function convert(::Type{Rational{Int64}}, x::Int16)
    Rational{Int64}(Int64(x), Int64(1))
end

function convert(::Type{Rational{Int64}}, x::Int8)
    Rational{Int64}(Int64(x), Int64(1))
end

function convert(::Type{Rational{Int64}}, x::Bool)
    Rational{Int64}(x ? Int64(1) : Int64(0), Int64(1))
end

# Integer → Rational{Int32}
function convert(::Type{Rational{Int32}}, x::Int32)
    Rational{Int32}(x, Int32(1))
end

function convert(::Type{Rational{Int32}}, x::Int16)
    Rational{Int32}(Int32(x), Int32(1))
end

function convert(::Type{Rational{Int32}}, x::Int8)
    Rational{Int32}(Int32(x), Int32(1))
end

function convert(::Type{Rational{Int32}}, x::Bool)
    Rational{Int32}(x ? Int32(1) : Int32(0), Int32(1))
end

# Integer → Rational{Int16}
function convert(::Type{Rational{Int16}}, x::Int16)
    Rational{Int16}(x, Int16(1))
end

function convert(::Type{Rational{Int16}}, x::Int8)
    Rational{Int16}(Int16(x), Int16(1))
end

function convert(::Type{Rational{Int16}}, x::Bool)
    Rational{Int16}(x ? Int16(1) : Int16(0), Int16(1))
end

# Integer → Rational{Int8}
function convert(::Type{Rational{Int8}}, x::Int8)
    Rational{Int8}(x, Int8(1))
end

function convert(::Type{Rational{Int8}}, x::Bool)
    Rational{Int8}(x ? Int8(1) : Int8(0), Int8(1))
end

# Rational{BigInt} identity and cross-type conversions (Issue #2497)
function convert(::Type{Rational{BigInt}}, x::Rational{BigInt})
    x
end

function convert(::Type{Rational{BigInt}}, x::Rational{Int64})
    Rational{BigInt}(big(x.num), big(x.den))
end

function convert(::Type{Rational{BigInt}}, x::Rational{Int32})
    Rational{BigInt}(big(x.num), big(x.den))
end

function convert(::Type{Rational{BigInt}}, x::Rational{Int16})
    Rational{BigInt}(big(x.num), big(x.den))
end

function convert(::Type{Rational{BigInt}}, x::Rational{Int8})
    Rational{BigInt}(big(x.num), big(x.den))
end

function convert(::Type{Rational{BigInt}}, x::Rational)
    Rational{BigInt}(big(x.num), big(x.den))
end

# Integer → Rational{BigInt}
function convert(::Type{Rational{BigInt}}, x::BigInt)
    Rational{BigInt}(x, big(1))
end

function convert(::Type{Rational{BigInt}}, x::Int64)
    Rational{BigInt}(big(x), big(1))
end

function convert(::Type{Rational{BigInt}}, x::Int32)
    Rational{BigInt}(big(x), big(1))
end

function convert(::Type{Rational{BigInt}}, x::Int16)
    Rational{BigInt}(big(x), big(1))
end

function convert(::Type{Rational{BigInt}}, x::Int8)
    Rational{BigInt}(big(x), big(1))
end

function convert(::Type{Rational{BigInt}}, x::Bool)
    Rational{BigInt}(big(x ? 1 : 0), big(1))
end

# Rational → Float64 (any Rational type)
function convert(::Type{Float64}, x::Rational)
    Float64(x.num) / Float64(x.den)
end

# Rational → Float32 (any Rational type)
function convert(::Type{Float32}, x::Rational)
    Float32(x.num) / Float32(x.den)
end

# =============================================================================
# zero(::Type) for numbers
# =============================================================================
# Based on Julia's base/number.jl:
#   zero(::Type{T}) where {T<:Number} = convert(T, 0)

function zero(::Type{T}) where {T<:Number}
    convert(T, 0)
end

# =============================================================================
# Promotion-based arithmetic operators (fallback for mixed types)
# =============================================================================
# Based on Julia's base/promotion.jl:
#   +(x::Number, y::Number) = +(promote(x,y)...)
#
# These fallbacks handle mixed-type arithmetic (e.g., Float32 + Int64)
# by promoting both operands to a common type via promote(), then
# dispatching to the concrete same-type operator.
#
# Specificity guarantees correct dispatch priority:
#   +(::Int64, ::Int64)       -> score 30 (concrete, always wins)
#   +(::Float64, ::Float64)   -> score 30 (concrete, always wins)
#   +(::Int64, ::Rational{T}) -> score 19 (parametric, wins over Number)
#   +(::Number, ::Number)     -> score 2  (abstract, last resort)
#
# See Julia's base/promotion.jl for reference implementation.

# Arithmetic
function Base.:(+)(x::Number, y::Number)
    px, py = promote(x, y)
    px + py
end

function Base.:(-)(x::Number, y::Number)
    px, py = promote(x, y)
    px - py
end

function Base.:(*)(x::Number, y::Number)
    px, py = promote(x, y)
    px * py
end

function Base.:(/)(x::Number, y::Number)
    px, py = promote(x, y)
    px / py
end

function Base.:(^)(x::Number, y::Number)
    px, py = promote(x, y)
    px ^ py
end

# Comparisons
function Base.:(==)(x::Number, y::Number)
    px, py = promote(x, y)
    px == py
end

function Base.:(<)(x::Real, y::Real)
    px, py = promote(x, y)
    px < py
end

function Base.:(<=)(x::Real, y::Real)
    px, py = promote(x, y)
    px <= py
end

# > and >= via promotion (Issue #2094: needed for mixed-type comparisons)
# In Julia, >(x, y) = y < x and >=(x, y) = y <= x
function Base.:(>)(x::Real, y::Real)
    px, py = promote(x, y)
    px > py
end

function Base.:(>=)(x::Real, y::Real)
    px, py = promote(x, y)
    px >= py
end

function Base.max(x::Real, y::Real)
    px, py = promote(x, y)
    if py < px
        px
    else
        py
    end
end

function Base.min(x::Real, y::Real)
    px, py = promote(x, y)
    if py < px
        py
    else
        px
    end
end

function Base.minmax(x::Real, y::Real)
    px, py = promote(x, y)
    if py < px
        (py, px)
    else
        (px, py)
    end
end
