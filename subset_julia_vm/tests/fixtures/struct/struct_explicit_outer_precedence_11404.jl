# A source-written explicit where-parametric outer constructor participates
# in dispatch even when the call also matches the automatic field
# constructor: upstream lets ExplicitOuterGap{T}(x::T) where {T} replace the
# synthetic default inner, so ExplicitOuterGap{Int}(1) calls the user method
# instead of constructing (Issue #11404, tech-debt #11447). Non-matching or
# different-arity user outers keep the default field constructor reachable.
using Test
# 1. plain parametric construction untouched
struct Plain{T}
    x::T
end
@test Plain{Int}(1).x === 1
@test Plain(2.5).x === 2.5

# 2. user outer with non-matching signature: default field ctor wins
struct Q11404{T}
    x::T
end
Q11404{T}(s::String) where {T} = Q11404{T}(length(s))
@test Q11404{Int}(1).x === 1
@test Q11404{Int}("abc").x === 3

# 3. delegating outer with different arity
struct R11404{T}
    v::T
end
R11404{T}(x::T, y::T) where {T} = R11404{T}(x + y)
@test R11404{Int}(2, 3).v === 5
@test R11404{Int}(7).v === 7

# 4. same-signature outer replaces the synthetic default (the MWE)
struct ExplicitOuterGap{T}
    x::T
end
ExplicitOuterGap{T}(x::T) where {T} = nothing
@test ExplicitOuterGap(1) isa ExplicitOuterGap{Int}
@test ExplicitOuterGap{Int}(1) === nothing
true
