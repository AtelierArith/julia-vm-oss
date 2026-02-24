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
# Integer promotion rules
# =============================================================================

# Bool promotes to any integer type
function promote_rule(::Type{Int64}, ::Type{Bool})
    Int64
end

function promote_rule(::Type{Int32}, ::Type{Bool})
    Int32
end

function promote_rule(::Type{Int16}, ::Type{Bool})
    Int16
end

function promote_rule(::Type{Int8}, ::Type{Bool})
    Int8
end

# Smaller integers promote to larger integers
function promote_rule(::Type{Int64}, ::Type{Int32})
    Int64
end

function promote_rule(::Type{Int64}, ::Type{Int16})
    Int64
end

function promote_rule(::Type{Int64}, ::Type{Int8})
    Int64
end

function promote_rule(::Type{Int32}, ::Type{Int16})
    Int32
end

function promote_rule(::Type{Int32}, ::Type{Int8})
    Int32
end

function promote_rule(::Type{Int16}, ::Type{Int8})
    Int16
end

# =============================================================================
# Unsigned integer promotion rules (based on julia/base/int.jl:775-788)
# =============================================================================
# Signed integers promote unsigned integers of smaller or equal size
# Based on: promote_rule(::Type{Int64}, ::Union{..., Type{UInt16}, Type{UInt32}, Type{UInt8}}) = Int64

# Int64 promotes UInt8, UInt16, UInt32
function promote_rule(::Type{Int64}, ::Type{UInt8})
    Int64
end

function promote_rule(::Type{Int64}, ::Type{UInt16})
    Int64
end

function promote_rule(::Type{Int64}, ::Type{UInt32})
    Int64
end

# Int32 promotes UInt8, UInt16
function promote_rule(::Type{Int32}, ::Type{UInt8})
    Int32
end

function promote_rule(::Type{Int32}, ::Type{UInt16})
    Int32
end

# Int16 promotes UInt8
function promote_rule(::Type{Int16}, ::Type{UInt8})
    Int16
end

# Int128 promotes all unsigned types
function promote_rule(::Type{Int128}, ::Type{UInt8})
    Int128
end

function promote_rule(::Type{Int128}, ::Type{UInt16})
    Int128
end

function promote_rule(::Type{Int128}, ::Type{UInt32})
    Int128
end

function promote_rule(::Type{Int128}, ::Type{UInt64})
    Int128
end

# Same-size signed/unsigned: promote to unsigned (julia/base/int.jl:784-788)
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

# Bool promotes to unsigned integers
function promote_rule(::Type{UInt8}, ::Type{Bool})
    UInt8
end

function promote_rule(::Type{UInt16}, ::Type{Bool})
    UInt16
end

function promote_rule(::Type{UInt32}, ::Type{Bool})
    UInt32
end

function promote_rule(::Type{UInt64}, ::Type{Bool})
    UInt64
end

function promote_rule(::Type{UInt128}, ::Type{Bool})
    UInt128
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
# Complex promotion rules
# =============================================================================

# Complex{Float64} with Complex{Int64} -> Complex{Float64}
function promote_rule(::Type{Complex{Float64}}, ::Type{Complex{Int64}})
    Complex{Float64}
end

# Complex{Float64} with Complex{Bool} -> Complex{Float64}
function promote_rule(::Type{Complex{Float64}}, ::Type{Complex{Bool}})
    Complex{Float64}
end

# Complex{Int64} with Complex{Bool} -> Complex{Int64}
function promote_rule(::Type{Complex{Int64}}, ::Type{Complex{Bool}})
    Complex{Int64}
end

# Complex{Float32} with Complex types
function promote_rule(::Type{Complex{Float64}}, ::Type{Complex{Float32}})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Float32}}, ::Type{Complex{Int64}})
    Complex{Float32}
end

function promote_rule(::Type{Complex{Float32}}, ::Type{Complex{Bool}})
    Complex{Float32}
end

# Real with Complex{Bool} -> Complex{Real}
function promote_rule(::Type{Complex{Bool}}, ::Type{Int64})
    Complex{Int64}
end

function promote_rule(::Type{Complex{Bool}}, ::Type{Float64})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Bool}}, ::Type{Float32})
    Complex{Float32}
end

# Real with Complex{Int64} -> Complex{...}
function promote_rule(::Type{Complex{Int64}}, ::Type{Int64})
    Complex{Int64}
end

function promote_rule(::Type{Complex{Int64}}, ::Type{Float64})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Int64}}, ::Type{Float32})
    Complex{Float32}
end

# Real with Complex{Float64} -> Complex{Float64}
function promote_rule(::Type{Complex{Float64}}, ::Type{Int64})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Float64}}, ::Type{Float64})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Float64}}, ::Type{Float32})
    Complex{Float64}
end

# Real with Complex{Float32}
function promote_rule(::Type{Complex{Float32}}, ::Type{Int64})
    Complex{Float32}
end

function promote_rule(::Type{Complex{Float32}}, ::Type{Float64})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Float32}}, ::Type{Float32})
    Complex{Float32}
end

# Bool with Complex types (Issue #2257)
function promote_rule(::Type{Complex{Float64}}, ::Type{Bool})
    Complex{Float64}
end

function promote_rule(::Type{Complex{Int64}}, ::Type{Bool})
    Complex{Int64}
end

function promote_rule(::Type{Complex{Bool}}, ::Type{Bool})
    Complex{Bool}
end

function promote_rule(::Type{Complex{Float32}}, ::Type{Bool})
    Complex{Float32}
end

# =============================================================================
# Rational promotion rules (explicit for each Integer subtype)
# =============================================================================
# Based on Julia's base/rational.jl:221-223
# Note: Generic where {T<:Integer, S<:Integer} patterns would be ideal, but
# promote_type dispatch has ambiguity issues with parametric types in the VM.
# Using explicit methods for each supported Integer type instead.

# Rational{Int64} + Integer types → Rational{Int64}
function promote_rule(::Type{Rational{Int64}}, ::Type{Int64})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Int32})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Int16})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Int8})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Bool})
    Rational{Int64}
end

# Rational{Int32} + Integer types → Rational{promote_type(Int32, S)}
function promote_rule(::Type{Rational{Int32}}, ::Type{Int64})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Int32})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Int16})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Int8})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Bool})
    Rational{Int32}
end

# Rational{Int16} + Integer types → Rational{promote_type(Int16, S)}
function promote_rule(::Type{Rational{Int16}}, ::Type{Int64})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Int32})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Int16})
    Rational{Int16}
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Int8})
    Rational{Int16}
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Bool})
    Rational{Int16}
end

# Rational{Int8} + Integer types → Rational{promote_type(Int8, S)}
function promote_rule(::Type{Rational{Int8}}, ::Type{Int64})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Int32})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Int16})
    Rational{Int16}
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Int8})
    Rational{Int8}
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Bool})
    Rational{Int8}
end

# Rational{T} + Rational{S} → Rational{promote_type(T,S)}
function promote_rule(::Type{Rational{Int64}}, ::Type{Rational{Int32}})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Rational{Int16}})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Rational{Int8}})
    Rational{Int64}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Rational{Int16}})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Rational{Int8}})
    Rational{Int32}
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Rational{Int8}})
    Rational{Int16}
end

# Rational{T} + Float types → Float (Julia semantics: float wins over rational)
function promote_rule(::Type{Rational{Int64}}, ::Type{Float64})
    Float64
end

function promote_rule(::Type{Rational{Int64}}, ::Type{Float32})
    Float32
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Float64})
    Float64
end

function promote_rule(::Type{Rational{Int32}}, ::Type{Float32})
    Float32
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Float64})
    Float64
end

function promote_rule(::Type{Rational{Int16}}, ::Type{Float32})
    Float32
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Float64})
    Float64
end

function promote_rule(::Type{Rational{Int8}}, ::Type{Float32})
    Float32
end

# Rational + BigInt → Rational{BigInt} (Issue #2497)
# Based on Julia's promote_rule(::Type{Rational{T}}, ::Type{S}) where {T<:Integer, S<:Integer}
function promote_rule(::Type{Rational{Int64}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int32}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int16}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{Int8}}, ::Type{BigInt})
    Rational{BigInt}
end

# Rational{BigInt} + other Rational types → Rational{BigInt}
function promote_rule(::Type{Rational{BigInt}}, ::Type{Rational{Int64}})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Rational{Int32}})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Rational{Int16}})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Rational{Int8}})
    Rational{BigInt}
end

# Rational{BigInt} + integer types → Rational{BigInt}
function promote_rule(::Type{Rational{BigInt}}, ::Type{BigInt})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Int64})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Int32})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Int16})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Int8})
    Rational{BigInt}
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Bool})
    Rational{BigInt}
end

# Rational{BigInt} + Float types → Float (Issue #2497)
function promote_rule(::Type{Rational{BigInt}}, ::Type{Float64})
    Float64
end
function promote_rule(::Type{Rational{BigInt}}, ::Type{Float32})
    Float32
end

# =============================================================================
# BigInt promotion rules (Issue #2512)
# Based on Julia's promote_rule(::Type{BigInt}, ::Type{<:Integer}) = BigInt
# =============================================================================

# BigInt promotes all integer types
function promote_rule(::Type{BigInt}, ::Type{Int64})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{Int32})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{Int16})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{Int8})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{Int128})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{Bool})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{UInt8})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{UInt16})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{UInt32})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{UInt64})
    BigInt
end
function promote_rule(::Type{BigInt}, ::Type{UInt128})
    BigInt
end

# BigInt + Float types → BigFloat
function promote_rule(::Type{BigInt}, ::Type{Float64})
    BigFloat
end
function promote_rule(::Type{BigInt}, ::Type{Float32})
    BigFloat
end
function promote_rule(::Type{BigInt}, ::Type{Float16})
    BigFloat
end

# =============================================================================
# BigFloat promotion rules (Issue #2512)
# Based on Julia's promote_rule(::Type{BigFloat}, ::Type{<:Real}) = BigFloat
# =============================================================================

# BigFloat promotes all numeric types
function promote_rule(::Type{BigFloat}, ::Type{Float64})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Float32})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Float16})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Int64})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Int32})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Int16})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Int8})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Int128})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{Bool})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{BigInt})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{UInt8})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{UInt16})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{UInt32})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{UInt64})
    BigFloat
end
function promote_rule(::Type{BigFloat}, ::Type{UInt128})
    BigFloat
end

# =============================================================================
# promote_type - find common type using promote_rule
# =============================================================================

function promote_type(::Type{T}, ::Type{S}) where {T, S}
    R1 = promote_rule(T, S)
    R2 = promote_rule(S, T)
    # Check both directions
    if R1 !== Union{}
        return R1
    elseif R2 !== Union{}
        return R2
    else
        return Any
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
# promote - convert values to common type
# =============================================================================

# Same-type fast path: no conversion needed when both args have the same type.
# This prevents unnecessary promote_type/convert calls and, crucially,
# avoids infinite recursion when Number fallback operators call promote
# on already-promoted (same-type) values.
function promote(x::T, y::T) where {T}
    (x, y)
end

function promote(x, y)
    T = promote_type(typeof(x), typeof(y))
    # Use intermediate variables to work around tuple construction bug
    # when function call results are directly used as tuple elements
    cx = convert(T, x)
    cy = convert(T, y)
    (cx, cy)
end

# 3-argument version: convert all three to common type
function promote(x, y, z)
    T = promote_type(typeof(x), typeof(y), typeof(z))
    # Use intermediate variables to work around tuple construction bug
    cx = convert(T, x)
    cy = convert(T, y)
    cz = convert(T, z)
    (cx, cy, cz)
end

# =============================================================================
# convert implementations
# =============================================================================

# Identity conversion
function convert(::Type{T}, x::T) where {T}
    x
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
    Int64(floor(x))
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
# zero for various types (needed for Complex conversion)
# =============================================================================

function zero(::Type{Int64})
    Int64(0)
end

function zero(::Type{Int32})
    Int32(0)
end

function zero(::Type{Int16})
    Int16(0)
end

function zero(::Type{Int8})
    Int8(0)
end

function zero(::Type{Float64})
    0.0
end

function zero(::Type{Float32})
    Float32(0.0)
end

function zero(::Type{Bool})
    false
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
