# Tuple covariance with user-defined parametric element types (Issue #5064).
# A tuple value holding a user struct must satisfy `isa` against Tuple{Foo{Int}},
# Tuple{Foo}, and Tuple{Any}, and dispatch must route covariantly element-wise,
# matching upstream Julia's `subtype_tuple`.

using Test

struct Foo{T}
    x::T
end

abstract type Animal end
struct Dog <: Animal end

# Covariant dispatch over a tuple element that is a user parametric type.
describe(::Tuple{Foo{Int}}) = "tuple of Foo{Int}"
describe(::Tuple{Foo}) = "tuple of Foo"
describe(::Tuple{Any}) = "tuple of Any"

@testset "Tuple isa with user parametric element" begin
    v = (Foo(1),)
    @test typeof(v) === Tuple{Foo{Int64}}
    @test v isa Tuple{Foo{Int}}
    @test v isa Tuple{Foo{Int64}}
    @test v isa Tuple{Foo}
    @test v isa Tuple{Any}
    @test !(v isa Tuple{Foo{Float64}})
    @test !(v isa Tuple{Foo{Int}, Int})

    # Multi-element covariance mixing user types and primitives.
    w = (Foo(1), 2)
    @test w isa Tuple{Foo{Int}, Int}
    @test w isa Tuple{Any, Real}
    @test !(w isa Tuple{Foo{Int}, String})
end

@testset "Tuple subtype with user parametric element" begin
    @test Tuple{Foo{Int}} <: Tuple{Foo{Int}}
    @test Tuple{Foo{Int}} <: Tuple{Foo}
    @test Tuple{Foo{Int}} <: Tuple{Any}
    @test !(Tuple{Foo{Int}} <: Tuple{Foo{Float64}})
    @test Tuple{Foo{Int64}} <: Tuple{Foo{Int}}
end

@testset "Tuple covariant dispatch over user parametric element" begin
    @test describe((Foo(1),)) == "tuple of Foo{Int}"
end

@testset "Tuple isa with user abstract element" begin
    d = (Dog(),)
    @test d isa Tuple{Animal}
    @test d isa Tuple{Dog}
    @test d isa Tuple{Any}
end

true
