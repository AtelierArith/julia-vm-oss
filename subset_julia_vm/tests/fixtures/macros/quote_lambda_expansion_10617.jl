# Issue #10617: arrow lambdas (`Expr(:->, ...)`) inside a macro's quote body.
# The dynamic macro-expansion engine (`macro_runtime.rs`, used by every
# user-defined/bundled/Base macro and stdlib macros in expression position)
# lifts the lambda (`arrow_expr_from_values`), and its parameter is
# scope-registered for hygiene rename via the Issue #10925 scope-aware
# RenameEnv — renamed only within the lambda's own params+body, never in an
# unrelated sibling reference sharing the bare name. Typed parameters go
# through the same `function_param_from_value` reader as a named function
# definition. (The static stdlib engine's arrow arm is unit-tested in
# `lowering/expr/quote/code_generation.rs`, since fixture-defined macros
# always take the dynamic engine.)

# --- Case 1: the exact MWE from Issue #10617's body -----------------------
# julia 1.12.6: 2
macro m_lambda_mwe()
    quote
        f = x -> x + 1
        f(1)
    end
end
check_lambda_mwe = @m_lambda_mwe() == 2

# --- Case 2: multi-parameter tuple lambda ----------------------------------
# julia 1.12.6: 12
macro m_lambda_two_params()
    quote
        g = (a, b) -> a * b
        g(3, 4)
    end
end
check_lambda_two_params = @m_lambda_two_params() == 12

# --- Case 3: typed single parameter ----------------------------------------
# julia 1.12.6: 42
macro m_lambda_typed_param()
    quote
        h = (x::Int) -> x + 1
        h(41)
    end
end
check_lambda_typed_param = @m_lambda_typed_param() == 42

# --- Case 4: lambda param sharing a bare name with a sibling GLOBAL call ---
# (mirror of the #10925/#10626 `sort` regression-guard pattern)
# julia 1.12.6: (11, 6) — the lambda's own `sort` param is scope-local; the
# sibling `sort([3, 1, 2])` call still resolves to Base.sort.
macro m_lambda_sibling_sort()
    quote
        f = sort -> sort + 1
        (f(10), sum(sort([3, 1, 2])))
    end
end
check_lambda_sibling_sort = @m_lambda_sibling_sort() == (11, 6)

# --- Case 5: lambda param shadowing a macro-introduced quote-local ---------
# julia 1.12.6: (6, 10) — the param collapses onto the local's gensym but the
# call frame shadows it, so the local keeps its value after the call.
macro m_lambda_shadows_quote_local()
    quote
        x = 10
        f = x -> x * 2
        (f(3), x)
    end
end
check_lambda_shadows_local = @m_lambda_shadows_quote_local() == (6, 10)

# --- Case 6: lambda param colliding with a CALLER variable -----------------
# julia 1.12.6: (8, 5) — the caller's `y` is untouched by the expansion.
y = 5
macro m_lambda_caller_collision()
    quote
        f = y -> y + 1
        f(7)
    end
end
check_lambda_caller_collision = (@m_lambda_caller_collision(), y) == (8, 5)

check_lambda_mwe &&
    check_lambda_two_params &&
    check_lambda_typed_param &&
    check_lambda_sibling_sort &&
    check_lambda_shadows_local &&
    check_lambda_caller_collision
