# Issue #7302: a type exported by a module is reachable unqualified (via
# `export`) but the *qualified* form `Module.Type` previously failed to
# resolve. The qualified access `M.Circle` was mis-routed as a module
# *function* lookup and errored with "Module M has no function named Circle".
# Upstream Julia always allows `Module.Type` for concrete, abstract, and
# parametric types — in `isa`, as a `::Module.T` annotation (including a
# qualified abstract type in a method signature), as a `Type{Module.T}`
# dispatch target, and as a bare type value.

using Test

module M
abstract type Shape end
struct Circle <: Shape
    r::Float64
end
struct Wrapped{T}
    v::T
end
mutable struct Counter
    n::Int
end
export Shape, Circle, Wrapped, Counter
end
using .M

# qualified abstract type in a method parameter annotation (resolved to the
# module's abstract `Shape`, so a `M.Circle` argument dispatches here)
describe(s::M.Shape) = "a shape"
# qualified concrete type in a method parameter annotation
area(c::M.Circle) = 3.14 * c.r * c.r
# qualified return-type annotation
make_circle()::M.Circle = M.Circle(1.0)
# Type{Module.T} dispatch
which_type(::Type{M.Circle}) = "circle type"
which_type(::Type{M.Counter}) = "counter type"

@testset "qualified Module.Type access (Issue #7302)" begin
    c = M.Circle(2.0)
    # qualified concrete type in isa
    @test isa(c, M.Circle)
    # qualified abstract type in isa
    @test isa(c, M.Shape)
    # qualified concrete is a subtype of qualified abstract
    @test M.Circle <: M.Shape
    # qualified type as value: same object as the unqualified exported type
    @test M.Circle === Circle
    # qualified annotation `::Module.T`
    c2 = M.Circle(3.0)::M.Circle
    @test c2.r == 3.0
    # qualified parametric type
    w = M.Wrapped(5)
    @test isa(w, M.Wrapped)
    # qualified mutable struct type
    cnt = M.Counter(0)
    @test isa(cnt, M.Counter)
    # `x isa Module.T` infix form
    @test c isa M.Circle
    # qualified abstract type in a method parameter annotation
    @test describe(c) == "a shape"
    # qualified concrete type in a method parameter annotation
    @test area(M.Circle(2.0)) == 3.14 * 4.0
    # qualified return-type annotation
    @test make_circle().r == 1.0
    # Type{Module.T} dispatch
    @test which_type(M.Circle) == "circle type"
    @test which_type(M.Counter) == "counter type"
end

true
