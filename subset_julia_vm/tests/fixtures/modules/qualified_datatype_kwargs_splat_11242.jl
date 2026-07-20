# A qualified DataType called through a runtime Module value must not fall back
# to an unrelated same-named function while expanding args/kwargs (Issue #11242).
module QualifiedDatatypeOwner11242
export Forwarded11242

struct Forwarded11242
    left
    right
end

struct Inner11242
    value
    Inner11242() = new(42)
end
end

module QualifiedDatatypeFacade11242
import ..QualifiedDatatypeOwner11242

Forwarded11242(args...; kwargs...) =
    QualifiedDatatypeOwner11242.Forwarded11242(args...; kwargs...)
Inner11242(args...; kwargs...) =
    QualifiedDatatypeOwner11242.Inner11242(args...; kwargs...)

export Forwarded11242, Inner11242
end

using .QualifiedDatatypeFacade11242

value11242 = Forwarded11242(1, 2)
println(value11242)
println(value11242.left + value11242.right)
println(Inner11242().value)
true
