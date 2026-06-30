# Issue #7304: `Vector{T}(undef, n)` and the typed-array literal `T[...]` must
# preserve a USER-STRUCT element type `T`, not widen it to `Any`.
#
# sjulia stored a `Vector{Any}` for `Vector{PP}(undef, 1)` and `PP[PP(1)]` because
# the constructor/literal element-type derivation only recognized builtin primitive
# type names and fell through to `ArrayElementType::Any` for any user struct. The
# `StructOf(type_id)` element tag (already resolved back to the struct name by
# reflection via `struct_defs`) is now produced for registered user structs.
#
# Builtin primitive eltypes must NOT regress (still `Vector{Int64}`, etc.).
#
# Verified against upstream Julia 1.12.6.

using Test

struct ProductPP7304
    x::Int
end

mutable struct CounterMM7304
    n::Int
end

@testset "Vector{T}(undef, n) preserves a user-struct eltype (Issue #7304)" begin
    v = Vector{ProductPP7304}(undef, 1)
    @test typeof(v) === Vector{ProductPP7304}
    @test eltype(v) === ProductPP7304

    m = Vector{CounterMM7304}(undef, 2)
    @test typeof(m) === Vector{CounterMM7304}
    @test eltype(m) === CounterMM7304
end

@testset "typed-array literal T[...] preserves a user-struct eltype (Issue #7304)" begin
    v = ProductPP7304[ProductPP7304(1)]
    @test typeof(v) === Vector{ProductPP7304}
    @test eltype(v) === ProductPP7304
    @test v[1].x == 1

    w = ProductPP7304[ProductPP7304(1), ProductPP7304(2)]
    @test typeof(w) === Vector{ProductPP7304}
    @test length(w) == 2
    @test w[2].x == 2

    c = CounterMM7304[CounterMM7304(5)]
    @test typeof(c) === Vector{CounterMM7304}
    @test eltype(c) === CounterMM7304
end

@testset "builtin primitive eltypes do NOT regress (Issue #7304)" begin
    @test typeof(Vector{Int}(undef, 1)) === Vector{Int64}
    @test typeof(Vector{Int8}(undef, 2)) === Vector{Int8}
    @test typeof(Vector{Float32}(undef, 2)) === Vector{Float32}
    @test typeof(Vector{String}(undef, 2)) === Vector{String}
    @test typeof(Int[1, 2]) === Vector{Int64}
    @test typeof(Int8[Int8(1)]) === Vector{Int8}
    @test typeof(Float32[1.0f0]) === Vector{Float32}
    @test eltype(Vector{Any}(undef, 1)) === Any
    @test typeof(Any[1, 2]) === Vector{Any}
end

true
