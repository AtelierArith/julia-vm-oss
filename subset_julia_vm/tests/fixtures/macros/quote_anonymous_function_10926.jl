# Issue #10926: anonymous `function (params...) ... end` expressions
# (`Expr(:function, Expr(:tuple, ...), body)`) returned from a macro quote,
# on the dynamic expansion path (`macro_runtime.rs`,
# `anonymous_function_expr_from_values`): the body is lifted as a lambda and
# the expression yields the function value, mirroring the arrow-lambda arm.
# The NAMED form already worked (statement path, `function_stmt_from_values`).
# Parameters are hygiene-registered scope-aware (#10925 RenameEnv, `Tuple`
# signature arm of `collect_function_def_param_and_where_names`).

# --- Case 1: the MWE from Issue #10926's body -------------------------------
# julia 1.12.6: 3
macro m_anon_mwe()
    quote
        f = function (x)
            x + 1
        end
        f(2)
    end
end
check_anon_mwe = @m_anon_mwe() == 3

# --- Case 2: hygiene-parity fixture promised in PR #10913 / Issue #10926 ---
# A macro-introduced local `x` + an anonymous-fn parameter `x`.
# julia 1.12.6: (2, 10) — the parameter shadows only inside the function;
# the quote-local keeps its value.
macro m_anon_hygiene_parity()
    quote
        x = 10
        f = function (x)
            x + 1
        end
        (f(1), x)
    end
end
check_anon_hygiene_parity = @m_anon_hygiene_parity() == (2, 10)

# --- Case 3: multiple parameters --------------------------------------------
# julia 1.12.6: 13
macro m_anon_multi_param()
    quote
        g = function (a, b)
            a * b + 1
        end
        g(3, 4)
    end
end
check_anon_multi_param = @m_anon_multi_param() == 13

# --- Case 4: typed parameter (upstream signature shape
#     `Expr(:tuple, Expr(:(::), :n, :Int))` — the quote constructor wraps the
#     single typed parameter in the one-element tuple upstream produces) ----
# julia 1.12.6: 41
macro m_anon_typed_param()
    quote
        h = function (n::Int)
            n - 1
        end
        h(42)
    end
end
check_anon_typed_param = @m_anon_typed_param() == 41

# --- Case 5: parameter sharing a bare name with a sibling GLOBAL call ------
# (mirror of the #10925/#10626 `sort` regression-guard pattern)
# julia 1.12.6: (15, 6)
macro m_anon_sibling_sort()
    quote
        f = function (sort)
            sort + 5
        end
        (f(10), sum(sort([3, 1, 2])))
    end
end
check_anon_sibling_sort = @m_anon_sibling_sort() == (15, 6)

check_anon_mwe &&
    check_anon_hygiene_parity &&
    check_anon_multi_param &&
    check_anon_typed_param &&
    check_anon_sibling_sort
