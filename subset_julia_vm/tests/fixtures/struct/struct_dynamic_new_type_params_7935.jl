using Test

module M7935
export UniversalRing, MyRing, MyElem, elem_type, coefficient_ring
abstract type Ring end
abstract type RingElem end
struct MyElem <: RingElem end
struct MyRing <: Ring end
elem_type(::MyRing) = MyElem
coefficient_ring(::MyRing) = MyRing()
mutable struct UniversalRing{T <: RingElem, U} <: Ring
    base_ring::Ring
    function UniversalRing(R::Ring)
        return new{elem_type(R), elem_type(coefficient_ring(R))}(R)
    end
end
end
using .M7935

@testset "Issue #7935: dynamic type params in inner constructor new{...}" begin
    r = UniversalRing(MyRing())
    ps = typeof(r).parameters
    @test length(ps) == 2
    @test ps[1] === MyElem
    @test ps[2] === MyElem
    @test ps[1] !== Any
    @test nameof(typeof(r)) === :UniversalRing
    @test r.base_ring isa MyRing
end

true
