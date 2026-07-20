# Issue #11014: an escaped/interpolated identifier at a function-signature
# BINDING position inside a macro's quote — the parameter's own name, its type
# annotation, and a `where`-bound type-variable name — must reconstruct to that
# bare name (the escaped identifier resolves at the macro call site and is never
# hygiene-renamed), exactly like upstream Julia.
#
# Verified against `julia --startup-file=no`.

# 1. Escaped parameter NAME at the binding position.
macro m_esc_param(pname)
    quote
        function esc_param_fn($(esc(pname)))
            $(esc(pname)) + 1
        end
        esc_param_fn(9)
    end
end

@assert @m_esc_param(caller_local) == 10

# 2. Escaped identifier as a parameter TYPE ANNOTATION.
macro m_esc_type(tname)
    quote
        function esc_type_fn(x::$(esc(tname)))
            x * 2
        end
        esc_type_fn(5)
    end
end

@assert @m_esc_type(Int) == 10

# 3. Escaped identifier as a `where`-BOUND type-variable name (used both as the
#    binder and as the parameter's type annotation).
macro m_esc_where(tvar_name)
    quote
        function esc_where_fn(x::$(esc(tvar_name))) where $(esc(tvar_name))
            x
        end
        esc_where_fn(11)
    end
end

@assert @m_esc_where(MyEscT) == 11

# 4. Interaction with scope-aware quote hygiene (Issue #10925): an ESCAPED
#    parameter name is NOT hygiene-renamed, while the quote's own locals still
#    are — the body sees both the escaped parameter and the renamed quote-local.
#    (The special case where the escaped parameter name COLLIDES with a
#    same-named quote-local — upstream then lets the parameter shadow it — is
#    tracked separately as Issue #11107 and deliberately not asserted here.)
macro m_esc_param_vs_hygiene(pname)
    quote
        quote_local = 100
        function esc_hyg_fn($(esc(pname)))
            $(esc(pname)) + quote_local
        end
        esc_hyg_fn(1)
    end
end

@assert @m_esc_param_vs_hygiene(val) == 101

# 5. A non-escaped parameter still shadows a caller binding of the same name
#    (hygiene renaming keeps the quote's own parameter separate from the
#    caller's global).
plain_param = 7

macro m_plain_param()
    quote
        function plain_param_fn(plain_param)
            plain_param * 3
        end
        plain_param_fn(4)
    end
end

@assert @m_plain_param() == 12
@assert plain_param == 7

# 6. Escaped parameter name combined with a typed, non-escaped sibling.
macro m_esc_param_typed_sibling(pname)
    quote
        function esc_mixed_fn($(esc(pname)), y::Int)
            $(esc(pname)) * 10 + y
        end
        esc_mixed_fn(2, 3)
    end
end

@assert @m_esc_param_typed_sibling(lhs) == 23

println("macro esc signature binding (Issue #11014) OK")
true
