# Issue #10610: a macro-introduced, non-escaped helper call must resolve in
# the macro's DEFINITION module (upstream hygiene semantics), independent of
# what the caller imported — `using .M: @m` imports only the macro, yet the
# expansion's `helper()` still calls `M.helper`. The dynamic engine's
# module-member qualification (`MacroHygieneInfo`, Issue #7355) provides
# this; this fixture is the regression guard for the exact Issue MWE plus
# the caller-shadow / esc / qualified-control variants.

module M10610
helper() = 1
helper_add(x) = x + 1
inner(x) = 2x
combo(x) = inner(x) + 1
macro m_plain()
    :(helper())
end
macro m_esc()
    esc(:(helper()))
end
macro m_qualified()
    :(M10610.helper())
end
macro m_nested(ex)
    :(helper_add(combo($(esc(ex)))))
end
export helper_add
end

using .M10610: @m_plain, @m_esc, @m_qualified, @m_nested

# --- Case 1: the exact MWE — helper NOT imported by the caller -------------
# julia 1.12.6: 1
check_definition_module = @m_plain() == 1

# --- Case 2: caller defines a same-named function; the non-escaped
#     expansion still resolves in M10610, the esc'd one in caller scope ----
# julia 1.12.6: plain -> 1, esc -> 100
helper() = 100
check_caller_shadow_plain = @m_plain() == 1
check_caller_shadow_esc = @m_esc() == 100

# --- Case 3: explicitly qualified control -----------------------------------
# julia 1.12.6: 1
check_qualified = @m_qualified() == 1

# --- Case 4: helper with arguments + nested helper-to-helper calls, used
#     both at top level and inside a function body --------------------------
# julia 1.12.6: 8 and 12
g10610() = @m_nested 3
check_nested_helpers_in_function = g10610() == 8
check_nested_helpers_toplevel = (@m_nested 5) == 12

check_definition_module &&
    check_caller_shadow_plain &&
    check_caller_shadow_esc &&
    check_qualified &&
    check_nested_helpers_in_function &&
    check_nested_helpers_toplevel
