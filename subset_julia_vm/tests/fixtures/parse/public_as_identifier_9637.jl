using Test

# Issue #9637: `public` is a *contextual* keyword — it is only special as a
# statement introducer (`public foo, bar`). Everywhere else (macro/function
# names, ordinary variables, parameters, struct fields, call targets) it must
# parse as an ordinary identifier, exactly as upstream Julia does.

# (1) Macro definition named `public` — the form that regressed in MacroTools.
module MPublic
    macro public(ex)
        esc(ex)
    end
    @public f() = 1
end

# (2) Long-form function definition named `public`.
function public(x)
    x + 1
end

# (3) Short-form function definition named `public`.
public() = 100

# (4) `public` as a struct field name.
struct Holder
    public::Int
end

# (5) `public` as a parameter name.
addone(public) = public + 1

@testset "`public` as ordinary identifier (Issue #9637)" begin
    # Macro named `public` expands correctly.
    @test MPublic.f() == 1

    # Long-form `function public(x) ... end`.
    @test public(4) == 5

    # Short-form `public() = ...`.
    @test public() == 100

    # Function value bound to a variable, then called.
    g = public
    @test g(9) == 10

    # `public` as a struct field name.
    @test Holder(7).public == 7

    # `public` as a parameter name.
    @test addone(10) == 11

    # `public` as an ordinary local variable.
    let public = 42
        @test public == 42
    end
end

# `public` as a statement introducer still works at top level.
module MPublicStmt
    public foo, bar
    foo() = 1
    bar() = 2
end
@test MPublicStmt.foo() == 1
@test MPublicStmt.bar() == 2

true
