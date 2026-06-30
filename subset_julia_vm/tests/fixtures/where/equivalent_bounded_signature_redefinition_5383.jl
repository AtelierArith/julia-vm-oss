# Issue #5383 (sub-case 3): `h(x::T) where {T<:Number}` and `h(x::Number)` are
# the same signature upstream (`Tuple{T} where T<:Number == Tuple{Number}`), so
# the later definition *redefines* the earlier one (last-definition-wins). The
# VM previously kept both as distinct methods and resolved the tie by
# registration order, so it returned the first-registered body instead of the
# last.
#
# The fix canonicalizes a method's structured signature for dedup: a covariant
# `where` variable used exactly once as a whole top-level parameter collapses to
# its bound, so the two spellings dedup and the later one replaces the earlier.
# Diagonal (`f(x::T, y::T)`) and invariant-nested (`Vector{T}`) uses are
# preserved, so genuinely distinct methods are never merged.

using Test

# Bounded-typevar form defined first, concrete form second → concrete wins.
h1(x::T) where {T<:Number} = :bounded
h1(x::Number) = :concrete

# Concrete form first, bounded-typevar form second → bounded wins.
h2(x::Number) = :concrete
h2(x::T) where {T<:Number} = :bounded

@testset "equivalent bounded-signature redefinition (Issue #5383)" begin
    # Direct (statically dispatched) calls.
    @test h1(5) == :concrete
    @test h2(5) == :bounded

    # Runtime dispatch via an `Any`-typed container element.
    v = Any[5]
    @test h1(v[1]) == :concrete
    @test h2(v[1]) == :bounded
end

true
