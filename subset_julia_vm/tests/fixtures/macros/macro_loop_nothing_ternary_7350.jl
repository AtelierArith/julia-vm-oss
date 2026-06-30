# Issue #7350 (B5): a `nothing`-initialised accumulator reassigned inside a loop
# via a heterogeneous-typed ternary (`acc === nothing ? x : g(acc, x)`) must
# accumulate across iterations. The slot was narrowed to `Nothing` and the
# `=== nothing` guard const-folded to always-true, dropping the back-edge value.
function fold_strs()
    acc = nothing
    for x in [1, 2, 3]
        acc = acc === nothing ? "s$(x)" : "$(acc)_$(x)"
    end
    acc
end

function fold_exprs()
    acc = nothing
    for x in [1, 2, 3]
        acc = acc === nothing ? x : Expr(:call, :+, acc, x)
    end
    acc
end

fold_strs() == "s1_2_3" &&
    fold_exprs() == Expr(:call, :+, Expr(:call, :+, 1, 2), 3)
