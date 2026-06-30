# Issue #5005: ::Module parameter must win dispatch specificity over an
# untyped parameter when both methods match.

using Test

@testset "::Module wins specificity over untyped parameter (Issue #5005)" begin
    foo(m::Module, s::Symbol) = "module-form"
    foo(x, s::Symbol) = "generic-form"
    @test foo(Base, :sum) == "module-form"
    @test foo(Core, :Int) == "module-form"
    @test foo(Main, :x) == "module-form"
    @test foo(42, :y) == "generic-form"   # untyped method still reachable

    # Reverse declaration order must not change the winner.
    bar(x, s::Symbol) = "generic-bar"
    bar(m::Module, s::Symbol) = "module-bar"
    @test bar(Base, :sum) == "module-bar"
    @test bar("str", :y) == "generic-bar"

    # Module beats Any in a single-argument shape too.
    baz(m::Module) = "module-baz"
    baz(x) = "generic-baz"
    @test baz(Base) == "module-baz"
    @test baz(3.0) == "generic-baz"

    # Module's runtime type is exactly Module.
    @test typeof(Base) === Module
    @test isa(Base, Module)
end

true
