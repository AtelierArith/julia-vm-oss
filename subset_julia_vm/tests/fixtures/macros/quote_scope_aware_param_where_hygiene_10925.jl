# Issue #10925 (follow-up to #10626, PR #10913): the dynamic macro-expansion
# hygiene rename (`rename_quote_local_symbols` / `macro_runtime.rs`) is now
# scope-aware (a stack of rename frames pushed/popped at `function`
# definitions and `where` clauses) instead of a flat whole-tree
# substitution, so a macro-quoted function's PARAMETER names and `where`
# type-parameter names can finally be registered for hygiene rename safely
# -- matching upstream Julia's own `@macroexpand` behavior -- without the
# regression #10626 found: registering them under the OLD flat mechanism
# renamed every occurrence of the bare name anywhere in the expansion,
# including an unrelated sibling reference outside the introducing
# function. See docs/vm/LOWERING.md's "Converging the Two Engines' Pass-1
# Decision Table" section and memory/project/project_10626_macro_hygiene_all_forms.md.

# --- Case 1: the exact regression MWE from Issue #10925's body -----------
# A function parameter sharing a bare name with a sibling GLOBAL function
# call. julia 1.12.6: (11, [1, 2, 3]) -- the parameter is hygiene-renamed
# within f's own body, but the sibling `sort([3, 1, 2])` call still resolves
# to Base.sort.
macro m_sibling_sort_param()
    quote
        function f(sort)
            sort + 1
        end
        (f(10), sort([3, 1, 2]))
    end
end
check_sibling_sort_param = @m_sibling_sort_param() == (11, [1, 2, 3])

# --- Case 2: where-binder shadowing a builtin type name -------------------
# julia 1.12.6: `function g(x::Vector) where Vector; (x, Vector); end` turns
# `x::Vector` into a parametric constraint (the where-bound `Vector` shadows
# the builtin type only within g's own signature+body); g([1,2,3]) returns
# ([1, 2, 3], Vector{Int64}).
macro m_where_shadows_builtin_type()
    quote
        function g(x::Vector) where Vector
            (x, Vector)
        end
        g([1, 2, 3])
    end
end
check_where_shadow = @m_where_shadows_builtin_type() == ([1, 2, 3], Vector{Int64})

# A sibling reference to the real, global `Vector` type outside the
# where-shadowed function must be unaffected by the shadow: `[3, 4] isa
# Vector` must still test against the genuine builtin type, not the
# gensym'd TypeVar `h`'s own where-clause introduces (which would make
# `isa` behave differently, or error, if the shadow incorrectly leaked).
macro m_where_shadow_sibling_unaffected()
    quote
        function h(x::Vector) where Vector
            x
        end
        (h([1, 2]), [3, 4] isa Vector)
    end
end
check_where_shadow_sibling = @m_where_shadow_sibling_unaffected() == ([1, 2], true)

# --- Case 3: nested functions sharing a parameter name --------------------
# The inner function's parameter shadows the enclosing function's own
# same-named parameter; sjulia's function-call-frame scoping keeps them
# functionally independent regardless of hygiene renaming.
# julia 1.12.6: 21.
macro m_nested_function_param_shadow()
    quote
        function outer(y)
            function inner(y)
                y + 1
            end
            inner(y) + y
        end
        outer(10)
    end
end
check_nested_shadow = @m_nested_function_param_shadow() == 21

# --- Case 4: a closure captures an outer macro-local from inside a nested -
# --- function (NOT its own parameter) -------------------------------------
macro m_closure_captures_outer_local()
    quote
        acc = 100
        function bump()
            acc + 1
        end
        (bump(), acc)
    end
end
check_closure_capture = @m_closure_captures_outer_local() == (101, 100)

# --- Case 5: a standalone `where` (not attached to a function definition) -
# introduces a scope for its own bound type-variable, but that rename does
# NOT leak into an unrelated sibling reference of the same bare name --
# the standalone counterpart of Case 1's sibling-parameter guard. Checked
# via `isa`, equality, and both subtype directions. Issue #11013 previously
# made the latter checks false because macro hygiene gives the binder a `#`-
# containing gensym spelling; alias recognition now alpha-projects the runtime
# TypeVar identity before comparing the generic wrapper with `Vector`.
macro m_standalone_where_no_leak()
    quote
        S = Vector{T} where T
        (
            isa([1, 2, 3], S),
            S <: AbstractVector,
            S == Vector,
            Vector == S,
            Vector <: S,
            T([1, 2, 3]),
        )
    end
end
T(xs) = length(xs)
check_standalone_where = @m_standalone_where_no_leak() == (true, true, true, true, true, 3)

# --- esc() still works for the function-parameter form -------------------
# Escaping the function's own NAME (leaving its parameter plain) still makes
# it caller-visible and callable, while the plain parameter is still
# correctly hygiene-renamed within its own body -- proving the new
# Function-scope-push logic does not interfere with an escaped sibling
# position within the SAME Function node it is scoping.
macro m_esc_function_name_plain_param()
    quote
        function $(esc(:esc_visible_fn))(sort)
            sort + 1
        end
    end
end
@m_esc_function_name_plain_param()
check_esc_function_name = esc_visible_fn(9) == 10 && sort([3, 1, 2]) == [1, 2, 3]

# --- esc() still works for the where-clause form --------------------------
# An esc()'d reference used INSIDE a where-shadowed function body must
# still resolve in the CALLER's scope, unaffected by the where-clause's own
# introduced scope for its bound variable.
outer_val_for_esc_where = 999
macro m_esc_ref_inside_where_shadowed_fn()
    quote
        function wfn(x::Vector) where Vector
            $(esc(:outer_val_for_esc_where))
        end
        wfn([1, 2, 3])
    end
end
check_esc_ref_in_where = @m_esc_ref_inside_where_shadowed_fn() == 999

check_sibling_sort_param &&
    check_where_shadow &&
    check_where_shadow_sibling &&
    check_nested_shadow &&
    check_closure_capture &&
    check_standalone_where &&
    check_esc_function_name &&
    check_esc_ref_in_where
