# An immediately-applied anonymous arrow lambda `(x -> body)(arg)` used inside a
# function body (not as the function's last expression) must lower to a normal,
# value-yielding call — NOT a tail `return` of the enclosing function. Previously
# the lifted lambda's call was wrapped in `Stmt::Return`, so the `return` leaked
# into the enclosing frame: `r = (x -> body)(arg)` returned the lambda's value
# early (skipping the continuation) or failed to compile with
# "Cannot convert Nothing to I64" when the result was used downstream
# (Issue #8018). A named binding of the same lambda, and the same IIFE at top
# level, always worked.

function closures_iife_continue_after_8018()
    # MWE 1: continuation after the IIFE returns a bool. The IIFE result must be
    # bound to `r` and the final `&&` expression evaluated as the return value.
    w = [10.0, 20.0]
    r = (x -> x[2])(w)
    (w == [10.0, 20.0]) && (r == 20.0)
end

function closures_iife_arithmetic_8018()
    # MWE 2: arithmetic on the IIFE result. `r` must hold the lambda's value (6),
    # not Nothing, so `r * 10` yields 60.
    r = (x -> x + 1)(5)
    y = r * 10
    return y
end

function closures_iife_chain_8018()
    # Several IIFEs in sequence, each feeding the next, then combined.
    a = (x -> x[1])([1, 2, 3])
    b = (x -> x + 1)(a)
    c = (t -> t * 2)(b)
    return a + b + c
end

function closures_iife_named_equiv_8018()
    # The named-binding form (always worked) must still agree with the IIFE form.
    g = x -> x + 1
    named = g(5) * 10
    iife = (x -> x + 1)(5) * 10
    named == iife
end

ok1 = closures_iife_continue_after_8018()        # true
ok2 = closures_iife_arithmetic_8018() == 60
ok3 = closures_iife_chain_8018() == 7            # 1 + 2 + 4
ok4 = closures_iife_named_equiv_8018()

ok1 && ok2 && ok3 && ok4
