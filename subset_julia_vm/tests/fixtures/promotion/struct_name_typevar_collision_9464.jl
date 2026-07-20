using Test

# Issue #9464: a declared struct/abstract type whose name matches the
# type-variable-name shape `^[A-Z][0-9]*$` (e.g. `S2`, `P3`, `W1`, `Q7`) used to
# be misclassified as an unbound type variable by the string-only
# `is_type_variable_name` heuristic. Along the same-type Real/Number operator
# dispatch and `convert` path this made the concrete same-type / same-family
# method lose specificity to the abstract `::Real` promote fallback, breaking
# dispatch (wrong `convert` selection / StackOverflow) — even though the
# IDENTICAL definition under a non-matching name (`Wrap`, `Foo`) worked.
#
# The fix registers every declared type name in a VM-local registry consulted by
# `is_type_variable_name`, so a registered type is never treated as a type
# variable regardless of spelling.
#
# Verified against upstream julia 1.12 (--startup-file=no).

# Parametric wrapper with a matching name `W1` + parametric promote_rule/convert
# and a same-type `+`, mirroring the ForwardDiff.Dual / Symbolics.Num pattern.
struct W1{T<:Real} <: Real
    v::T
end
Base.promote_rule(::Type{W1{T}}, ::Type{S}) where {T<:Real,S<:Real} = W1{promote_type(T, S)}
Base.convert(::Type{W1{T}}, x::Real) where {T<:Real} = W1{T}(convert(T, x))
Base.convert(::Type{W1{T}}, w::W1) where {T<:Real} = W1{T}(convert(T, w.v))
Base.:+(a::W1{T}, b::W1{T}) where {T<:Real} = W1{T}(a.v + b.v)

@testset "parametric matching-name struct dispatches via convert (Issue #9464)" begin
    @test promote_type(W1{Int64}, Float64) === W1{Float64}
    r = W1(1) + 2.5              # promotes W1{Int}→W1{Float64}, same-type + fires
    @test r isa W1{Float64}
    @test r.v == 3.5
    @test (W1(1.0) + W1(2.0)).v == 3.0
end

# Non-parametric matching-name struct `S2` with its own operator methods.
struct S2 <: Real
    a::Float64
end
Base.:+(x::S2, y::S2) = S2(x.a + y.a)
Base.:(==)(x::S2, y::S2) = x.a == y.a
Base.:<(x::S2, y::S2) = x.a < y.a
Base.:*(x::S2, y::S2) = S2(x.a * y.a)

@testset "non-parametric matching-name struct operator dispatch (Issue #9464)" begin
    @test (S2(1.0) + S2(2.0)).a == 3.0
    @test S2(1.0) == S2(1.0)
    @test S2(1.0) < S2(2.0)
    @test (S2(2.0) * S2(3.0)).a == 6.0
end

# Same-type user `+` on a matching-name struct reached through the cached Base
# `+(::Number, ::Number)` fallback (the original #9335 candidate-visibility path,
# with the original `P3` name restored now that #9464 is fixed).
struct P3 <: Real
    a::Float64
end
Base.:+(x::P3, y::P3) = P3(x.a + y.a)

@testset "matching-name same-type + via Base fallback (Issue #9464)" begin
    @test (P3(1.0) + P3(2.0)).a == 3.0
end

true
