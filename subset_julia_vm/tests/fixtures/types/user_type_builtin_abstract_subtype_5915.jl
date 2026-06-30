# Issue #5915 wave 3: the runtime `<:` (check_subtype) is now decided solely by
# the shared CoreSubtypeEngine over the VM struct hierarchy. A non-parametric
# user struct / abstract type whose declared parent is a BUILT-IN abstract must
# be a subtype of that abstract (and its transitive supertypes) — previously
# this was recovered by a separate type_ancestors fallback. A parametric user
# struct declaring a parametric abstract parent must match an existential
# `where` right operand while keeping invariant parameters. All answers verified
# against upstream julia 1.12.

struct Money <: Real end
abstract type Currency <: Number end
abstract type Wrapper{T} end
struct MyVec{T} <: Wrapper{T} end

# Non-parametric user struct -> built-in abstract chain.
@assert Money <: Real
@assert Money <: Number
@assert !(Money <: AbstractFloat)
@assert !(Money <: Integer)

# User abstract type -> built-in abstract chain.
@assert Currency <: Number
@assert !(Currency <: Real)

# Through tuples (covariant element-wise).
@assert Tuple{Money} <: Tuple{Number}
@assert Tuple{Money, Currency} <: Tuple{Real, Number}

# Parametric user struct -> existential parametric parent (binds S), but
# invariant against a concrete differing parameter.
@assert MyVec{Int64} <: (Wrapper{S} where S)
@assert !(MyVec{Int64} <: Wrapper{Real})
@assert MyVec{Int64} <: Wrapper{Int64}

println("ok")
true
