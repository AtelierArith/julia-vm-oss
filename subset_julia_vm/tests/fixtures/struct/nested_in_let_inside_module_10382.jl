# Issue #10382: a `struct` definition nested inside a `let`/`begin` block
# INSIDE a `module ... end` body must lower and run, matching upstream Julia
# — the narrower gap left after Issue #10194 fixed the same pattern at
# Program/file top level (and inside `Test.@testset`'s macro-expanded
# `let`). `lower_module_definition`'s statement-lowering catch-all only
# threaded the module's `LambdaContext` through when the statement itself
# contained a macro call (`contains_macro_call`), so a plain `let`/`begin`
# with no macro call anywhere fell back to the ctx-less path and hit
# sjulia's ordinary `UnsupportedFeature { kind:
# UnsupportedExpression("struct_definition") }` error — even though the
# identical statement at true top level already worked after #10194.

module M10382
let
    struct FooLetM10382
        x::Int
    end
    println(FooLetM10382(1).x)
end
end

# The struct is visible after the `let` ends too (module-scope binding),
# and qualified access through the module works.
r1 = M10382.FooLetM10382(2).x

# A `begin...end` wrapper (not just `let`) inside a module must also work,
# mirroring #10194's begin/let parity at true top level.
module N10382
begin
    struct FooBeginN10382
        y::Int
    end
end
end

r2 = N10382.FooBeginN10382(3).y

r1 == 2 && r2 == 3
