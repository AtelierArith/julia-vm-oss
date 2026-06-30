# Issue #6601: exercise `Expr.head` / `Expr.args` field-access used as an
# assignment RHS so the migrated FieldAccess slot typing (engine: head::Symbol,
# args::Vector{Any}) is exercised end-to-end. The locals `h`/`a` get their slot
# types from the field-access RHS; `describe` returns the head symbol and the
# arg count (4 — `:(g(1,2,3)).args` includes the callee `:g`).
function describe(ex)
    h = ex.head
    a = ex.args
    return (h, length(a))
end

e = :(g(1, 2, 3))
r = describe(e)
println(r[1])
println(r[2])
println(typeof(e.head))
println(typeof(e.args))

true
