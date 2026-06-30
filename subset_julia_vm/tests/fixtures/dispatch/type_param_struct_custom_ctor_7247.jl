using Test

# Issue #7247: `::Type{Foo}` dispatch must match when `Foo` is a PARAMETRIC type
# that ALSO has user-defined OUTER constructor methods. Passing the bare type
# `Foo` must resolve to the type object (`Type{Foo}`), not the constructor
# function (`typeof(Foo)`). A parametric struct lives in `parametric_structs`,
# not `struct_table`; its outer constructors register the name in the method
# tables, so without an explicit parametric-struct arm in the call-site type
# inference the bare reference was mis-typed as the constructor function and a
# `ff(::Type{Foo}, v)` method failed to match.
module D7247
abstract type AB end
struct Foo{T<:Real} <: AB
    x::T
end
Foo(a::Real, b::Real) = Foo{typeof(float(a))}(float(a))
Foo(a::Real) = Foo(a, 1.0)
ff(::Type{Foo}, v) = 99
gg(::Type{Foo}) = "type"
hh(x::Foo) = "instance"
export Foo, ff, gg, hh
end
using .D7247

@testset "Type{Foo} dispatch for parametric type with custom ctors (Issue #7247)" begin
    # The bare parametric type matches `::Type{Foo}`.
    @test ff(Foo, [5.0]) == 99
    @test gg(Foo) == "type"
    # Type identity still works.
    @test Foo === Foo
    # The custom outer constructors still build instances.
    f = Foo(5.0)
    @test f isa Foo
    @test f.x == 5.0
    @test hh(f) == "instance"
    # Two-arg custom ctor.
    f2 = Foo(3, 4)
    @test f2.x == 3.0
end

true
