# Issue #11019: a qualified alias whose leaf matches a constructor-self binder
# is still an alias; only the unqualified S inside its parameter list denotes
# the binder. The equivalent later definition replaces the first.

module AliasLeafOwner11019
const S = Vector
end

struct AliasLeafCollision11019{T}
    AliasLeafCollision11019{Vector{S}}() where S = :first
    AliasLeafCollision11019{AliasLeafOwner11019.S{S}}() where S = :second
end

ok = AliasLeafCollision11019{Vector{Int}}() == :second
println(ok)
ok
