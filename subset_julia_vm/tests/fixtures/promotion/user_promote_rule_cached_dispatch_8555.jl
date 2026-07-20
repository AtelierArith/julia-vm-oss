# User promote_rule extensions must dispatch correctly even when Base is
# served from the precompiled cache (Issue #8555 retires the #4048
# disable-the-whole-Base-cache fallback in favor of refreshing the cached
# bytecode's frozen dispatch candidate lists).
# Verified against upstream julia 1.12.

import Base: promote_rule, convert

struct Meters8555
    value::Float64
end

struct Feet8555
    value::Float64
end

# Concrete user-type/Base-type rule (both lookup directions must work).
promote_rule(::Type{Meters8555}, ::Type{Float64}) = Meters8555

# where-bounded rule anchored by the user type in the first slot.
promote_rule(::Type{Feet8555}, ::Type{T}) where {T<:Real} = Feet8555

# User-type/user-type rule.
promote_rule(::Type{Meters8555}, ::Type{Feet8555}) = Meters8555

convert(::Type{Meters8555}, x::Float64) = Meters8555(x)

ok = true

# promote_type consults user rules in both argument orders.
ok = ok && promote_type(Meters8555, Float64) === Meters8555
ok = ok && promote_type(Float64, Meters8555) === Meters8555
ok = ok && promote_type(Feet8555, Int64) === Feet8555
ok = ok && promote_type(Bool, Feet8555) === Feet8555
ok = ok && promote_type(Meters8555, Feet8555) === Meters8555
ok = ok && promote_type(Feet8555, Meters8555) === Meters8555

# Base-known pairs keep the Base rules.
ok = ok && promote_type(Int64, Float64) === Float64
ok = ok && promote_type(Int8, Int16) === Int16

# promote() converts through the user rule.
a, b = promote(Meters8555(1.5), 2.0)
ok = ok && a === Meters8555(1.5)
ok = ok && b === Meters8555(2.0)

ok
