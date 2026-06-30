# Issue #8352: a covariant/contravariant bound shorthand inside `{}` —
# `Foo{<:Bound}` / `Foo{>:Bound}` (sugar for `Foo{T} where T<:Bound`) — must
# lower as a static bounded type expression, not be (mis)classified as a dynamic
# value parameter and routed through expression lowering, where the prefix `<:`
# is rejected with `UnsupportedOperator("<:")`. Regression introduced by #8339's
# change to `is_dynamic_type_arg`.

using Test

@testset "covariant/contravariant bound type args (Issue #8352)" begin
    # Lowering must succeed (these threw a lowering error before the fix).
    @test Vector{<:Real} isa Type
    @test Array{>:Int} isa Type

    # The bound participates in subtype queries.
    @test Vector{Int} <: Vector{<:Real}
    @test Vector{Float64} <: Vector{<:Real}
    @test !(Vector{String} <: Vector{<:Real})
    @test !(Type{<:Real} <: DataType)

    # As a method-argument annotation (the common use of the shorthand).
    g8352(::Type{<:Real}) = :real
    g8352(::Type{<:AbstractString}) = :str
    @test g8352(Int) === :real
    @test g8352(Float64) === :real
    @test g8352(String) === :str
end

true
