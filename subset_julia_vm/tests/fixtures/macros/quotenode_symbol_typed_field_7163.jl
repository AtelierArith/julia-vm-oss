# Issue #7163: a macro that injects a symbol literal via `QuoteNode` into
# generated code constructing a struct with a `::Symbol`-typed field must
# deliver a genuine `Symbol`, not a value boxed as `Any`. Previously the
# macro-injected `Literal::Symbol` compiled to a `PushSymbol` that the
# compiler statically typed as `Any`, so the constructor field coercion
# failed at compile time with "Cannot convert Any to Symbol". The
# source-level `:sym` path (`QuoteLiteral(SymbolNew)`) already reported
# `Symbol`, which is why direct `Named(:alpha)` worked but the macro form
# did not.

struct Named
    name::Symbol
end

# Macro with an escaped argument splicing `QuoteNode(x)` into the constructor.
macro bind(x)
    Expr(:(=), esc(x), Expr(:call, :Named, QuoteNode(x)))
end

@bind alpha
println(alpha)
println(alpha.name)
println(alpha.name === :alpha)
println(typeof(alpha.name))

# Constant QuoteNode (no escaped argument) into the typed field.
macro make_beta()
    Expr(:call, :Named, QuoteNode(:beta))
end
b = @make_beta
println(b.name)
println(b.name === :beta)

# Macro-injected symbol into a function with a `::Symbol` parameter.
takes_symbol(s::Symbol) = s
macro call_takes()
    Expr(:call, :takes_symbol, QuoteNode(:gamma))
end
println(@call_takes)
println((@call_takes) === :gamma)

true
