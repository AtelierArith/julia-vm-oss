using Test

# Issue #5776: the empty NamedTuple literal `(;)` and the `NamedTuple()`
# constructor were unsupported. `(;)` parsed as a `ParameterList` in expression
# position (which `lower_expr` rejected), and `NamedTuple()` was not a registered
# callable. The semicolon-form named tuple `(; a=1, ...)` was unsupported too —
# only the comma form `(a=1, ...)` worked. Display also gains the empty
# `NamedTuple()` form and the single-field trailing comma.

@testset "empty NamedTuple literal and NamedTuple() (Issue #5776)" begin
    @test (;) == NamedTuple()
    @test NamedTuple() == (;)
    @test typeof((;)) == typeof(NamedTuple())
    @test length((;)) == 0
    @test isempty((;))

    nt = (; a=1, b=2)
    @test nt.a == 1
    @test nt.b == 2
    @test nt == (a=1, b=2)

    single = (; x=10)
    @test single.x == 10
    @test single == (x=10,)

    @test string((;)) == "NamedTuple()"
    @test string((; a=1)) == "(a = 1,)"
    @test string((; a=1, b=2)) == "(a = 1, b = 2)"
    @test string((a=1,)) == "(a = 1,)"
end

true
