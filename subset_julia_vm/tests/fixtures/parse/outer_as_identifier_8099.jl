using Test

# Issue #8099: `outer` is a *contextual* keyword — it is only special inside
# `for outer x in ...` (the outer-local-variable modifier). Everywhere else
# (function names, ordinary variables, parameters, struct fields, call targets)
# it must parse as an ordinary identifier, exactly as upstream Julia does.

# (1) Long-form function definition named `outer` — the form that regressed.
function outer(x)
    x + 1
end

# (2) Short-form function definition named `outer` (a second method, arity 0).
outer() = 100

# `outer` as a struct field name.
struct Holder
    outer::Int
end

# `outer` as a parameter name.
addone(outer) = outer + 1

@testset "`outer` as ordinary identifier (Issue #8099)" begin
    # Long-form `function outer(x) ... end`.
    @test outer(4) == 5

    # Short-form `outer() = ...`.
    @test outer() == 100

    # Function value bound to a variable, then called.
    f = outer
    @test f(9) == 10

    # `outer` as a struct field name.
    @test Holder(7).outer == 7

    # `outer` as a parameter name.
    @test addone(10) == 11

    # `outer` as an ordinary local variable.
    let outer = 42
        @test outer == 42
    end

    # Regression: in `for outer in itr` the `outer` is the loop *variable*
    # name, not the modifier — it must still bind normally (Issue #6414).
    s = 0
    for outer in 1:3
        s += outer
    end
    @test s == 6
end

true
