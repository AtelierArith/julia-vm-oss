# Issue #5374: a long-form `function ... where {T<:Bound} ... end` method dropped
# its bound, so the method matched argument/type values that do NOT satisfy the
# bound. The symptom surfaced via `eltype`: `eltype(UserStruct{T})` of a
# non-`Number` user parametric type returned the type itself instead of `Any`,
# because base's `function eltype(::Type{T}) where {T<:Number} = T` (long form)
# matched ANY `Type{X}`.
#
# Root cause: the pure parser builds the braced where-constraint `T<:Number` as a
# `BinaryExpression [T, <:, Number]` (operator is the middle named child). The
# long-form where-clause lowering read the bound from `children[1]` (= "<:")
# instead of the last child (= "Number"), so the stored upper bound became the
# bare operator and `usable_upper_bound("<:")` discarded it. Short-form `f(...) =
# expr` was unaffected because it lowers through `parse_subtype_expression`, which
# reads the last child.

using Test

struct UserP{T} end
struct Box{T} end

@testset "eltype of non-Number user parametric types (Issue #5374)" begin
    # The reported symptom: these returned the type itself, must be Any.
    @test eltype(UserP{String}) == Any
    @test eltype(UserP{Int64}) == Any
    @test eltype(Box{Float64}) == Any

    # Regression guard (#5365): Number parametric types remain their own eltype.
    @test eltype(Complex{Float64}) == Complex{Float64}
    @test eltype(Rational{Int64}) == Rational{Int64}

    # Scalars and containers unaffected.
    @test eltype(Float64) == Float64
    @test eltype(Int64) == Int64
    @test eltype(Vector{Int64}) == Int64
    @test eltype(Vector{UserP{Int64}}) == UserP{Int64}
end

# Direct root-cause check: a long-form braced `where {T<:Number}` method must
# reject types that are not subtypes of the bound, deferring to the fallback.
function tag_braced(::Type{T}) where {T<:Number}
    return :number
end
tag_braced(::Type) = :other

# Same for the unbraced `where T<:Number` long form (also lowered by full_form).
function tag_unbraced(::Type{T}) where T<:Number
    return :number
end
tag_unbraced(::Type) = :other

@testset "long-form where {T<:Number} on Type{T} respects bound (Issue #5374)" begin
    @test tag_braced(Float64) == :number
    @test tag_braced(Complex{Float64}) == :number
    @test tag_braced(UserP{Int64}) == :other
    @test tag_braced(String) == :other

    @test tag_unbraced(Float64) == :number
    @test tag_unbraced(UserP{Int64}) == :other
    @test tag_unbraced(String) == :other
end

# Note: value-position dispatch where a bounded typevar method (`f(x::T) where
# {T<:Number}`) competes with an untyped fallback (`f(x)`) currently mis-ranks
# specificity and is tracked separately — it is NOT caused by this parse bug
# (the short form, which lowers the bound correctly, exhibits it too). It is
# intentionally excluded here so this fixture stays scoped to the long-form
# bound-extraction fix.

true
