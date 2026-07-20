using Test

# Issue #9335 (with #9334 context): operator dispatch for user numeric types
# through the promote-fallback operators (`op(x, y) = op(promote(x, y)...)`).
#
# A user parametric `promote_rule` (e.g.
# `promote_rule(::Type{Wrap{T}}, ::Type{S})`) used to be invisible to the
# operator promote fallback: `promote_type` resolved correctly, but the
# `px + py` re-dispatch inside the cached Base `+(::Number, ::Number)` fallback
# (whose candidate list is frozen at Base-compile time, before any user method
# existed) could not see the user's `+(::Wrap{T}, ::Wrap{T})` method and looped
# forever -> StackOverflow. The fix augments same-type struct operand dispatch
# with the live name-indexed method table (plus a diagonal-dominance ranking
# for the augmented same-type candidates) in `vm/exec/binary_both.rs`.
#
# NOT covered here (still-open #9334 residual): upstream's same-type anchor
# methods from `julia/base/promotion.jl` (`==(x::T,y::T) where {T<:Number} =
# x === y`, `<`/`+`/... `no_op_err` anchors) are NOT ported — a user Real
# subtype with NO operator method of its own still StackOverflows instead of
# reaching egal `==` / a "not defined" error. Porting the anchors requires the
# dispatch rankers (runtime resolver AND compile-time router) to rank
# more-specific same-family methods (e.g. `<(::AbstractIrrational,
# ::AbstractIrrational)`, `+(x::T,y::T) where {T<:Union{Int64,...}}`) above a
# bare-typevar diagonal anchor, which they currently do not; see Issue #9334.
#
# Verified against upstream julia 1.12.
#
# NOTE (Issue #9464, now FIXED): the original report used `S2`/`P3` as the
# struct names. A name matching `^[A-Z][0-9]*$` used to collide with sjulia's
# `is_type_variable_name` string heuristic and StackOverflow via an unrelated,
# pre-existing bug. That bug is fixed (declared type names are registered and
# never treated as type variables), so the original `P3` name is restored here.

# User numeric wrapper with a parametric promote_rule + convert + same-type op,
# mirroring the ForwardDiff.Dual / Symbolics.Num / Unitful pattern (Issue #9335).
struct Wrap{T<:Real} <: Real
    v::T
end
Base.promote_rule(::Type{Wrap{T}}, ::Type{S}) where {T<:Real,S<:Real} = Wrap{promote_type(T, S)}
Base.convert(::Type{Wrap{T}}, x::Real) where {T<:Real} = Wrap{T}(convert(T, x))
Base.convert(::Type{Wrap{T}}, w::Wrap) where {T<:Real} = Wrap{T}(convert(T, w.v))
Base.:+(a::Wrap{T}, b::Wrap{T}) where {T<:Real} = Wrap{T}(a.v + b.v)

@testset "parametric promote_rule drives operator promotion (Issue #9335)" begin
    # promote_type resolves the user parametric rule in both directions.
    @test promote_type(Wrap{Int64}, Float64) === Wrap{Float64}
    @test promote_type(Float64, Wrap{Int64}) === Wrap{Float64}

    # Mixed Wrap{Int} + Float64: promotes to Wrap{Float64}, then the same-type
    # user `+` fires (previously StackOverflow: the promote fallback's frozen
    # candidate list, baked at Base-compile time, could not see this method).
    r = Wrap(1) + 2.5
    @test r isa Wrap{Float64}
    @test r.v == 3.5

    # Already-same-type user `+` still dispatches directly.
    r3 = Wrap(1.0) + Wrap(2.0)
    @test r3 isa Wrap{Float64}
    @test r3.v == 3.0
end

# A same-type user `+` reached through the cached Base `+(::Number, ::Number)`
# fallback must find the user method (Issue #9335 candidate-visibility gap).
struct P3 <: Real
    a::Float64
end
Base.:+(x::P3, y::P3) = P3(x.a + y.a)

@testset "user same-type + reached via Base fallback (Issue #9335)" begin
    @test (P3(1.0) + P3(2.0)).a == 3.0
end

@testset "ordinary numeric promotion is unaffected" begin
    @test (1 + 2.0) === 3.0
    @test (1 == 1.0)
    @test (1 < 2.0)
    @test (3 // 4 + 1 // 4) == 1
    @test (Complex(1.0, 2.0) == Complex(1, 2))
    @test max(1, 2.5) === 2.5
end

true
