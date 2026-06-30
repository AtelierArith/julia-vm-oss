# Test hasfield / hasproperty / propertynames for user types (Issue #5101)
#
# Upstream Julia (base/runtime_internals.jl):
#   hasfield(T::Type, name::Symbol) = fieldindex(T, name, false) > 0
#   propertynames(x) = fieldnames(typeof(x))
#   hasproperty(x, s::Symbol) = s in propertynames(x)
#
# The key behavior verified here is that `hasproperty` routes through the
# (overridable) `propertynames`, so a custom `propertynames` overload is
# honored even for property names that are not real fields.
#
# A single flat @testset is used so the testset summary matches upstream
# Test.jl for the fixture parity check (scripts/fixture_julia_parity.sh).

using Test

struct Foo
    x::Int
    y::Float64
end

struct Box{T}
    val::T
end

struct Empty
end

# Custom propertynames overload: hasproperty must honor it (Issue #5101 points 2/4)
struct Custom
    a::Int
    b::Int
end
Base.propertynames(::Custom) = (:a, :b, :virtual)

@testset "hasfield / hasproperty / propertynames (Issue #5101)" begin
    # plain struct
    foo = Foo(1, 2.0)
    @test hasfield(Foo, :x) === true
    @test hasfield(Foo, :y) === true
    @test hasfield(Foo, :z) === false
    @test propertynames(foo) === (:x, :y)
    @test hasproperty(foo, :x) === true
    @test hasproperty(foo, :y) === true
    @test hasproperty(foo, :z) === false

    # parametric struct
    b = Box{Int}(5)
    @test hasfield(Box{Int}, :val) === true
    @test hasfield(Box{Int}, :missing) === false
    @test propertynames(b) === (:val,)
    @test hasproperty(b, :val) === true
    @test hasproperty(b, :nope) === false

    # empty struct
    e = Empty()
    @test hasfield(Empty, :anything) === false
    @test propertynames(e) === ()
    @test hasproperty(e, :x) === false

    # custom propertynames overload honored by hasproperty
    c = Custom(1, 2)
    @test propertynames(c) === (:a, :b, :virtual)
    @test hasproperty(c, :a) === true
    @test hasproperty(c, :virtual) === true    # not a real field, but a property
    @test hasfield(Custom, :virtual) === false # still not a field
    @test hasproperty(c, :missing) === false
end

true
