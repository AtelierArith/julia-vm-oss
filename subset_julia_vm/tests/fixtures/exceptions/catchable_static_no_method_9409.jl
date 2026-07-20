using Test

# Issue #9409: a statically-proven no-method numeric call must raise a
# catchable *runtime* MethodError (matching upstream Julia), not abort the
# whole program with an uncatchable compile-time error. The generic
# indirection form (dynamic dispatch miss through a function-valued
# parameter) must be catchable too.

myop9409(f, a, b) = f(a, b)

@testset "statically no-method call is a catchable MethodError (Issue #9409)" begin
    # Direct binary form: compilation statically proves there is no
    # /(::Complex{Int64}, ::Char) method.
    e1 = try
        (2 + 3im) / 'a'
    catch e
        e
    end
    @test e1 isa MethodError
    @test typeof(e1) == MethodError

    # Generic indirection: the same miss through dynamic dispatch of a
    # function-valued argument.
    e2 = try
        myop9409(/, 2 + 3im, 'a')
    catch e
        e
    end
    @test e2 isa MethodError
    @test typeof(e2) == MethodError

    # Another statically no-method pair (non-Complex) for coverage.
    e3 = try
        'a' - 1.5
    catch e
        e
    end
    @test e3 isa MethodError

    # Execution continues after catching: genuine runtime errors still work.
    dz = try
        div(1, 0)
    catch e
        e
    end
    @test dz isa DivideError
end
println("after")
true
