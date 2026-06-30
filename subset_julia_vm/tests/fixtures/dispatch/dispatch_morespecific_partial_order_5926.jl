using Test

# Issue #5926 (part of #5072): method specificity now consults the upstream-style
# `morespecific` *partial order* (the subtype-decidable fragment) before the
# integer specificity score. The score alone mis-ranks several real relations, so
# these calls previously dispatched to the LESS specific method or raised a
# spurious ambiguity `MethodError`. Each result matches upstream Julia.
#
# The dominance override is gated on the argument tuple being a *subtype* of the
# chosen method's signature, so an imprecise (e.g. statically-`Any`) argument
# still falls through to runtime dispatch — see the "imprecise argument" set,
# which guards against the codegen-coupling regression the gate prevents.

# --- container vs. its abstract supertype: Vector{T} ≺ AbstractVector ---
cv(::AbstractVector) = :abstract
cv(::Vector{T}) where {T} = :vector

cm(::AbstractMatrix) = :abstract
cm(::Matrix{T}) where {T} = :matrix

# --- diagonal Tuple{T,T} ≺ Tuple{Any,Any} ---
diag2(::T, ::T) where {T} = :diagonal
diag2(::Any, ::Any) = :anyany

# --- bounded where-params: Vector{<:Integer} ≺ Vector{<:Real} ---
bnd(::Vector{T}) where {T<:Real} = :real
bnd(::Vector{T}) where {T<:Integer} = :integer

# --- nested parametric: Vector{Vector{T}} ≺ Vector{T} ---
nst(::Vector{T}) where {T} = :outer
nst(::Vector{Vector{T}}) where {T} = :nested

# NOTE: invariant parametric *struct* args (e.g. `Pair{T,T}` vs `Pair{A,B}`) are
# intentionally NOT covered here. A `Pair` value's element types are not tracked
# at runtime (its dispatch type is the bare `Pair`), so the morespecific override
# cannot — and must not — commit to the diagonal; that case is governed by the
# pre-existing score tie-breaker, independent of this change. Same for `Dict`.

@testset "container vs abstract supertype picks the concrete container" begin
    @test cv([1, 2, 3]) === :vector
    @test cm([1 2; 3 4]) === :matrix
end

@testset "diagonal Tuple{T,T} is more specific than Tuple{Any,Any}" begin
    @test diag2(1, 2) === :diagonal            # same type -> diagonal wins
    @test diag2(1, "x") === :anyany            # different types -> only Any,Any matches
end

@testset "bounded where-param picks the tighter bound (was a spurious ambiguity)" begin
    @test bnd([1, 2, 3]) === :integer
    @test bnd(Real[1.0, 2.0]) === :real
end

@testset "nested parametric is more specific than the shallow one" begin
    @test nst([[1], [2, 3]]) === :nested
    @test nst([1, 2, 3]) === :outer
end

# --- imprecise-argument guard (codegen-coupling regression prevention) ---
# When the static argument type is too coarse to *definitively* select the more
# specific method, the override must defer to runtime dispatch rather than commit
# to (and have codegen lower) a method the value may not satisfy. A `::Any`
# fallback called through an abstractly-typed container element still resolves
# correctly at runtime.
spec(::Int) = :int
spec(::Any) = :any

@testset "imprecise argument still dispatches correctly (no over-commit)" begin
    xs = Any[1, "two", 3.0]          # element static type is Any
    @test spec(xs[1]) === :int       # runtime value is Int -> Int method
    @test spec(xs[2]) === :any       # runtime value is String -> Any method
    # A generic higher-order call over a concrete collection must still lower and
    # run (this is the shape that regressed in earlier attempts).
    @test map(x -> 2x + 3, collect(1:5))[end] === 13
end

true
