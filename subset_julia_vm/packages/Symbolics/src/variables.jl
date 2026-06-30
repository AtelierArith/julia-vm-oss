# `@variables` macro (Issue #6572).
#
# Binds each name to `Num(Sym(:name))` in the caller's scope and returns a
# vector of the created `Num`s, a heavily reduced port of upstream
# `extern/Symbolics.jl/src/variable.jl` (whose `parse_vars` also handles type
# annotations, arrays and call syntax — out of scope for the core set).
#
# Caller-scope binding follows the bundled-package macro pattern proven by
# `Plots.@animate`/`@gif`: `esc` the assignment targets so the user's names
# resolve in the caller's scope, while macro-introduced names (`Num`, `Sym`)
# resolve in the defining module (`Symbolics`).
#
# Two subset-VM constraints shape the construction (vs. the more idiomatic
# `:( $(esc(x)) = Num(Sym($(QuoteNode(x)))) )` quasiquote):
#   * Splatting a vector into `Expr(head, args...)` inside a macro is currently
#     broken (Issue #7162), so the returned `:block` and `:vect` are grown with
#     `push!`.
#   * A macro-injected `QuoteNode` is boxed as `Any` (Issue #7163); the AST is
#     built explicitly with `Expr` and `Sym.name` is left untyped (see types.jl).

"""
    @variables x y z...

Declare symbolic variables. Each name is bound in the caller's scope to a
`Num` wrapping a `Sym`, and the macro returns a `Vector` of those `Num`s.

```julia
@variables x y
# x, y are now Num-wrapped symbolic variables; the macro also returns [x, y]
```
"""
macro variables(xs...)
    assigns = Expr(:block)
    vec = Expr(:vect)
    for x in xs
        rhs = Expr(:call, :Num, Expr(:call, :Sym, QuoteNode(x)))
        push!(assigns.args, Expr(:(=), esc(x), rhs))
        push!(vec.args, esc(x))
    end
    push!(assigns.args, vec)
    assigns
end
