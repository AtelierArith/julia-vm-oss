using Test

# Issue #5646: a PARAMETRIC user struct argument must dispatch to a method whose
# type parameter is bounded by a user abstract type (`f(x::T) where {T<:Shape}`).
# Non-parametric structs already worked (the standard binding resolver relates
# them to the bound), and a parametric instance already matched a plain abstract
# argument (`g(x::Shape)`), but the combination
# (parametric struct) × (where-bounded typevar) × (user abstract bound) failed
# with NoMethodFound. The struct-parent dispatch fallback was triggered (its gate
# has a TypeVar-bound arm) but the actual match check lacked the matching arm, and
# parametric user structs were absent from the `struct_parents` map (they live
# outside `struct_defs`, Issue #5052) so the chain could neither accept a real
# subtype nor reject an unrelated one.
#
# Rejection is checked via a catch-all fallback method (an unmatched call is a
# compile-time error in sjulia's static pipeline, not a catchable runtime
# MethodError), which also pins down WHICH method each argument selects.

abstract type Shape end
struct Circle{T<:Real} <: Shape
    r::T
end
struct Square <: Shape end
struct Box{T} end                 # NOT a Shape

abstract type Animal end
abstract type Mammal <: Animal end
struct Dog{T} <: Mammal
    name::T
end

abstract type Wrapper{T} end
struct MyVec{T} <: Wrapper{T}
    data::Vector{T}
end

f(x::T) where {T<:Shape} = "shape-bound"
f(x) = "fallback"
g(x::Shape) = "shape-arg"
g(x) = "g-fallback"
h(x::T) where {T<:Animal} = "animal-bound"
h(x) = "h-fallback"
w(x::T) where {T<:Wrapper} = "wrapper-bound"
w(x) = "w-fallback"

@testset "parametric struct dispatches to user-abstract-bounded method (Issue #5646)" begin
    # The bug: parametric struct argument against `where {T<:Shape}`.
    @test f(Circle(1.0)) == "shape-bound"
    @test f(Circle(2)) == "shape-bound"

    # Regressions that already worked: non-parametric struct, plain abstract arg.
    @test f(Square()) == "shape-bound"
    @test g(Circle(3.0)) == "shape-arg"
    @test g(Square()) == "shape-arg"

    # Multi-level user-abstract chain: Dog{T} -> Mammal -> Animal.
    @test h(Dog("rex")) == "animal-bound"

    # Parametric abstract parent: MyVec{T} <: Wrapper{T} matches bare Wrapper.
    @test w(MyVec([1, 2, 3])) == "wrapper-bound"
end

@testset "unrelated parametric struct selects the fallback, not the bound (Issue #5646)" begin
    # `Box{T}` has no declared parent, so it must NOT match `where {T<:Shape}`.
    @test f(Box{Int}()) == "fallback"
    @test g(Box{Int}()) == "g-fallback"
    # A Wrapper-bounded method must reject a Shape struct and vice versa.
    @test w(Circle(1.0)) == "w-fallback"
    @test f(MyVec([1, 2])) == "fallback"
end

true
