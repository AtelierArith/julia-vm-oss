# Issue #7900: tuple destructuring `a, b = f(x)` inside an esc'd macro body that is
# spliced into call-argument position was lowered as a CALL to the `=` operator
# (Runtime error: ErrorException: Unknown function: =) instead of a destructuring
# assignment. The arg constructor round-trips it as `Expr(:call, :(=),
# Expr(:tuple, a, b), rhs)`, so the macro-result→IR converter must special-case a
# tuple LHS the same way it already does a symbol LHS.

# A user macro that mirrors what @manipulate does: splice the esc'd body block into
# `push!(acc, <body>)` argument position.
macro mybuild(body)
    quote
        acc = Any[]
        push!(acc, $(esc(body)))
        acc
    end
end

f(k) = ([1.0 * k, 2.0 * k], [3.0 * k, 4.0 * k])

# Tuple destructuring in the spliced body block (non-tail statement).
spliced = @mybuild begin
    a, b = f(2)
    a .+ b
end
check_spliced = spliced == Any[[8.0, 12.0]]

# A simpler macro that just returns the esc'd body — exercises the
# value_to_stmt / value_to_branch_expr conversion path directly.
macro identity_body(body)
    esc(body)
end

# Destructuring from a function returning a tuple.
g(k) = (k, k + 1)
prod = @identity_body begin
    m, n = g(5)
    m * n
end
check_prod = prod == 30

# Three-element destructuring.
three = @identity_body begin
    x, y, z = (1, 2, 3)
    x * 100 + y * 10 + z
end
check_three = three == 123

# Nested tuple destructuring.
nested = @identity_body begin
    p, (q, r) = (1, (2, 3))
    p + q + r
end
check_nested = nested == 6

# Tuple assignment as the trailing (value-producing) expression of the body block:
# Julia evaluates an assignment expression to its RHS value.
tail = @identity_body begin
    w = 1
    s, t = (10, 20)
end
check_tail = tail == (10, 20)

check_spliced && check_prod && check_three && check_nested && check_tail
