using Test

# Issue #10942: a builtin-spelled `where` binder must shadow the builtin type
# name inside SIGNATURE annotations, so `Type{Float64}` lowers to `Type{T}`
# with `T` the binder and dispatch binds it to the actual type argument.

sig_type_10942(::Type{Float64}) where Float64 = Float64{Int64}

sig_nested_type_10942(x::Type{Vector{Float64}}) where Float64 = Float64

sig_bounded_type_10942(::Type{Float64}) where Float64<:Any = Float64

sig_vector_10942(x::Vector{Float64}) where Float64 = Float64

sig_union_10942(::Union{Float64, String}) where Float64 = Float64

function sig_full_form_10942(::Type{Float64}) where Float64
    Float64{Int64}
end

sig_tuple_10942(x::Tuple{Float64, Int64}) where Float64 = Float64

# Name-based shadowing only: an `Int` annotation under a `where Int64` binder
# still resolves to the builtin alias target; the binder shadows the spelling
# `Int64`, not the type it aliases.
sig_alias_negative_10942(x::Type{Int}) where {Int64} = 0

# Normal uses without a colliding binder are unaffected.
sig_plain_10942(::Type{Float64}) = 1
sig_plain_vector_10942(x::Vector{Float64}) = 2

@testset "builtin-spelled where binder dispatches in signature (Issue #10942)" begin
    @test sig_type_10942(Vector) == Vector{Int64}
    @test sig_type_10942(Set) == Set{Int64}

    @test sig_nested_type_10942(Vector{Int64}) === Int64
    @test sig_nested_type_10942(Vector{String}) === String

    @test sig_bounded_type_10942(String) === String

    @test sig_vector_10942([1, 2]) === Int64
    @test sig_vector_10942([1.0]) === Float64

    @test sig_union_10942(1) === Int64
    @test sig_union_10942(1.5) === Float64

    @test sig_full_form_10942(Dict) == Dict{Int64}

    @test sig_tuple_10942(("a", 1)) === String

    @test sig_alias_negative_10942(Int64) == 0

    @test sig_plain_10942(Float64) == 1
    @test sig_plain_vector_10942([1.0]) == 2
end

true
