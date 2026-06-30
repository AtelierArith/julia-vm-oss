# Issue #6502: runtime typed-dispatch (Type{T} pattern) selection must keep
# the most specific method when candidates are winnowed at runtime through
# the shared selection core (max-score, first wins on tie).
abstract type Marker6502 end
struct Tag6502 <: Marker6502 end

label6502(::Type{T}) where {T} = "generic"
label6502(::Type{Tag6502}) = "tag"

function pick6502(T::Type)
    label6502(T)
end

r1 = pick6502(Tag6502)
r2 = pick6502(Int64)
r1 == "tag" || error("expected tag, got $(r1)")
r2 == "generic" || error("expected generic, got $(r2)")
true
