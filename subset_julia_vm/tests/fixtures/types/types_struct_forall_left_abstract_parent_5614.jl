using Test

# Issue #5614: a forall-left where-form over a user-defined PARAMETRIC struct
# must resolve its declared abstract parent. `(Circle{T} where T) <: Shape` is
# `true` upstream because every `Circle{T}` instantiation declares `Shape` as its
# supertype, independent of `T`. sjulia previously reported it `false`: the
# rendered `where` operand is decided authoritatively by the structured `CoreType`
# solver (it never falls through to the runtime reflection table that already
# handles the brace-free `Circle{Int} <: Shape`), and that solver both lacked a
# `(Struct, Named)` arm AND never received parametric user structs in its
# struct-parent registry (they instantiate lazily and live outside `struct_defs`,
# Issue #5052).

abstract type Shape end
struct Circle{T<:Real} <: Shape
    r::T
end
struct Square <: Shape end

abstract type Animal end
abstract type Mammal <: Animal end
struct Dog{T} <: Mammal
    name::T
end

abstract type Wrapper{T} end
struct MyVec{T} <: Wrapper{T}
    data::Vector{T}
end

@testset "forall-left parametric struct resolves abstract parent (Issue #5614)" begin
    # The bug: explicit where-form over a parametric struct.
    @test (Circle{T} where T) <: Shape
    @test (Circle{T} where T <: Real) <: Shape
    @test (Circle{T} where T <: Integer) <: Shape

    # Regressions: the brace-free / concrete forms already worked.
    @test Circle <: Shape
    @test Circle{Int} <: Shape
    @test Square <: Shape

    # Multi-level chain through an intermediate user abstract type.
    @test (Dog{T} where T) <: Mammal
    @test (Dog{T} where T) <: Animal
    @test Dog{Int} <: Animal
    @test Dog <: Animal
end

@testset "forall-left parametric struct with a parametric abstract parent (Issue #5614)" begin
    # `struct MyVec{T} <: Wrapper{T}`: the bare UnionAll is a subtype of the bare
    # parametric abstract...
    @test (MyVec{T} where T) <: Wrapper
    @test MyVec{Int} <: Wrapper
    @test MyVec{Int} <: Wrapper{Int}

    # ...but element invariance still holds: not every `MyVec{T}` is a
    # `Wrapper{Int}`, and an unrelated abstract never matches.
    @test !((MyVec{T} where T) <: Wrapper{Int})
    @test !((MyVec{T} where T) <: Shape)
    @test !((Dog{T} where T) <: Wrapper)
    @test !((Circle{T} where T) <: Animal)
end

true
