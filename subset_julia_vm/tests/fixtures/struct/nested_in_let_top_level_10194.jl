# Issue #10194: a plain (non-macro) top-level `let ... struct Foo ... end
# ... end` must lower and run. `let` introduces a *local* scope for
# variables in upstream Julia, but a nested `struct`/`mutable struct`
# definition still binds at module scope regardless of that nesting — a
# `struct` is illegal only inside an actual function/closure body, not
# inside a `let`/`begin` wrapper reachable from top level. sjulia previously
# failed to lower this with
# `UnsupportedFeature { kind: UnsupportedExpression("struct_definition") }`.

r1 = let
    struct FooLet10194
        x::Int
    end
    FooLet10194(1).x
end

# The struct is visible after the `let` ends too (module-scope binding).
r2 = FooLet10194(2).x

# A mutable struct nested in `let` must also work, with field mutation.
r3 = let
    mutable struct MutableFooLet10194
        y::Int
    end
    m = MutableFooLet10194(1)
    m.y = 42
    m.y
end

r1 == 1 && r2 == 2 && r3 == 42
