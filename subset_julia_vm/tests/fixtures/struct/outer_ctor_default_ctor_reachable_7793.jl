# Regression test for Issue #7793: defining a user OUTER constructor (which
# registers the struct name as a function with a method table) must NOT make the
# synthesized field-count default constructor unreachable for a bare top-level
# call whose arity differs from the outer constructor.
#
# Before the fix, the field-count default-ctor call missed dispatch with
# `NoMethodFound` (its argument types degrade to Any once the name has methods,
# and the default ctor is not a method-table member). The fix adds the
# field-count default-constructor fallback to the multi-arg / static-miss
# `NoMethodFound` recovery arms (bare path) and to
# `compile_module_call_via_method_table` (qualified analog, same family as #7729).

using Test

struct Foo
    a::String
    b::Symbol
    c::Symbol
end
# Outer ctor of arity 2 (field count is 3) -> registers Foo as a function.
Foo(a::AbstractString, b::Symbol) = Foo(String(a), b, Symbol(""))

# A struct whose same-arity outer ctor has DIFFERENT types: both the default
# field constructor and the outer constructor must stay reachable.
struct P
    x::Int
    y::Int
end
P(a::String, b::String) = P(length(a), length(b))

# Recursion guard: a full-arity outer ctor whose body re-calls the field-count
# default constructor must NOT recurse forever.
struct W
    s::String
end
W(s::AbstractString) = W(String(s))

module M
struct Bar
    a::String
    b::Symbol
    c::Symbol
end
Bar(a::AbstractString, b::Symbol) = Bar(String(a), b, Symbol(""))
end
using .M

@testset "Outer ctor does not hide field-count default ctor (Issue #7793)" begin
    # Bare 3-arg call -> field-count default constructor.
    x = Foo("hi", :t, :u)
    @test x.a == "hi"
    @test x.b == :t
    @test x.c == :u

    # Bare 2-arg call -> outer constructor (its body calls the 3-arg default).
    y = Foo("hi", :t)
    @test y.c == Symbol("")

    # Same-arity overload: default field ctor and outer ctor both reachable.
    @test P(3, 4) == P(3, 4)
    p1 = P(3, 4)
    @test (p1.x, p1.y) == (3, 4)
    p2 = P("ab", "cde")
    @test (p2.x, p2.y) == (2, 3)

    # Full-arity outer ctor does not recurse.
    @test W("zz").s == "zz"

    # Qualified analog (module-qualified field-count default ctor call).
    z = M.Bar("hey", :p, :q)
    @test z.a == "hey"
    @test z.c == :q
end

true  # Test passed
