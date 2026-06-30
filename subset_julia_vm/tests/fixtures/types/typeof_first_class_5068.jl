# Issue #5068: Type{T} first-class treatment.
#
# Bounded `Type{<:B}` patterns, `Type{T} where T` binding, value-level
# subtype relations honoring `Type{T}` invariance, and `isa(x, Type{T})`
# for type-object values. Verified against upstream Julia.

using Test

# Bounded Type{<:Number} dispatch.
f(::Type{<:Number}) = "number-type"
f(::Type) = "any-type"

# Specificity: the tighter bound wins (no ambiguity).
k(::Type{<:Integer}) = "integer"
k(::Type{<:Number}) = "number"

# Type{T} where {T} binding.
g(::Type{T}) where {T} = "T=$T"

# Bounded where-var binding.
h(::Type{T}) where {T<:Real} = "real $T"

@testset "Issue #5068: Type{T} first-class treatment" begin
    # Bounded Type{<:B} dispatch.
    @test f(Int) == "number-type"
    @test f(Float64) == "number-type"
    @test f(String) == "any-type"

    # Specificity: the tighter bound wins (no ambiguity).
    @test k(Int) == "integer"
    @test k(Float64) == "number"

    # Type{T} where {T} and Type{T} where {T<:Real} binding.
    @test g(Int) == "T=Int64"
    @test g(String) == "T=String"
    @test h(Int) == "real Int64"

    # Type{T} subtype relations honoring invariance of the (concrete) inner.
    @test (Type{Int} <: Type{Int}) == true
    @test (Type{Int} <: Type{Integer}) == false
    @test (Type{Int} <: Type{<:Number}) == true
    @test (Type{String} <: Type{<:Number}) == false
    @test (Type{Int} <: Type) == true
    @test (DataType <: Type) == true

    # isa(x, Type{...}) for type-object values.
    @test (Int isa Type) == true
    @test (Int isa Type{Int}) == true
    @test (Int isa Type{Integer}) == false
    @test (Int isa Type{<:Number}) == true
    @test (Float64 isa Type{<:Integer}) == false
    @test (Vector{Int} isa Type{<:AbstractArray}) == true
    @test (3 isa Type{Int}) == false

    # zero/one dispatch via Type{T}.
    @test zero(Int) === 0
    @test zero(Float64) === 0.0
    @test one(Int) === 1
end

true
