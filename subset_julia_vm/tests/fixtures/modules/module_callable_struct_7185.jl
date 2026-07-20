# Issue #7185: a call operator (functor) `(obj::T)(args...)` defined inside a
# module is dispatched, whether the instance is applied from inside the module or
# from outside. The VM stores a module-qualified `struct_name` ("M.Foo"), but the
# functor method is registered under the bare `__callable_Foo`; the runtime
# lookup must strip the module path so the instance resolves to its method.
using Test

module M
    struct Foo; n; end
    (f::Foo)(y) = f.n + y
    callit(a, b) = Foo(a)(b)
end

module M2
    struct Foo; n; end
    (f::Foo)(y) = f.n + y
end

# Parametric callable struct inside a module.
module P
    struct Scale{T}; k::T; end
    (s::Scale)(x) = s.k * x
end

# Anonymous-self functor `(::Type)(args)` inside a module.
module A
    struct Doubler; end
    (::Doubler)(x) = 2x
    run(v) = Doubler()(v)
end

# A converting constructor combined with a functor on the same struct: the
# converting ctor runs (field becomes length), then the functor dispatches.
module M3
    struct Bar; v; end
    Bar(s::String) = Bar(length(s))
    (b::Bar)(y) = b.v + y
end

@testset "Issue #7185: module-defined callable struct dispatch" begin
    # Applied from inside the module.
    @test M.callit(10, 5) == 15
    # Applied from outside the module.
    z = M2.Foo(10)
    @test z(5) == 15
    # Parametric functor, both element types.
    @test P.Scale(3)(4) == 12
    @test P.Scale(2.0)(5) == 10.0
    # Anonymous-self functor, inside and outside.
    @test A.run(7) == 14
    @test A.Doubler()(9) == 18
    # Converting constructor + functor: "hello" -> length 5, then 5 + 3.
    @test M3.Bar("hello")(3) == 8
end

true
