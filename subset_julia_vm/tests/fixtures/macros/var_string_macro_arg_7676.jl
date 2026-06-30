# Issue #7676: var"name" identifiers passed to a macro inside a comma-grouped
# argument tuple must be preserved as Symbols (not String literals), and must
# round-trip back to var"name" in Julia-source form. Previously sjulia lowered
# var"@q" to the String "@q" when building the macro argument AST, producing
# Expr(:tuple, "@q", "@qq", :postwalk) instead of (var"@q", var"@qq", postwalk).
macro grabarg(ex)
    return QuoteNode(ex)
end

ex = @grabarg var"@q", var"@qq", postwalk

# Comma grouping (already correct before the fix) yields a single :tuple Expr.
@assert ex isa Expr
@assert ex.head == :tuple
@assert length(ex.args) == 3

# Representation: each element is a Symbol, not a String literal.
@assert ex.args[1] isa Symbol
@assert ex.args[2] isa Symbol
@assert ex.args[3] isa Symbol
@assert !(ex.args[1] isa String)
@assert !(ex.args[2] isa String)

# The var-string identifiers preserve their exact names.
@assert ex.args[1] === Symbol("@q")
@assert ex.args[2] === Symbol("@qq")
@assert ex.args[3] === :postwalk

# Julia-source round-trip: a Symbol that is not a valid identifier prints as
# var"name", so the quoted tuple stringifies exactly like upstream Julia.
@assert string(ex) == "(var\"@q\", var\"@qq\", postwalk)"

true
