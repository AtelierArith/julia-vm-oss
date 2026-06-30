# An immediately-applied FULL-FORM anonymous function `(function(x) body end)(arg)`
# used inside an enclosing function body must lift the lambda into a nested
# `FunctionDef` embedded in the `LetBlock` *body* so it is discovered/compiled as
# a nested function of the enclosing frame. Previously the no-`LambdaContext`
# body-lowering path routed this through `build_indirect_call`, burying the
# lambda's `FunctionDef` in a `LetBlock` *binding* the nested-function collector
# never visits, so `<parent>#__anonymous_function_N` had no bytecode and dispatch
# failed at runtime with "Function '...' not found" (Issue #8030). The arrow form
# `(x -> body)(arg)` and the same full-form IIFE at top level always worked.

function closures_fullform_iife_basic_8030()
    # MWE from the issue: full-form IIFE inside a function -> 6.
    (function(x) x + 1 end)(5)
end

function closures_fullform_iife_capture_8030()
    # The lifted lambda captures an outer local `a` from the enclosing frame.
    a = 10
    (function(x) x + a end)(5)
end

function closures_fullform_iife_two_params_8030()
    # Multiple parameters work through the same nested-lambda path.
    (function(x, y) x * y + 1 end)(3, 4)
end

function closures_fullform_iife_in_if_8030(b)
    # IIFE nested inside a control-flow branch is still collected as a nested fn.
    if b
        (function(x) x + 1 end)(5)
    else
        0
    end
end

function closures_fullform_iife_in_for_8030()
    s = 0
    for i in 1:3
        s += (function(x) x * x end)(i)
    end
    s
end

function closures_fullform_iife_with_macro_8030()
    # A macro call routes the enclosing function through the WITH-`LambdaContext`
    # lowering path; the full-form IIFE (capturing `a`) must work there too.
    a = 100
    @assert true
    (function(x) x + a end)(5)
end

function closures_fullform_iife_arrow_equiv_8030()
    # The arrow IIFE form (always worked) must agree with the full-form result.
    full = (function(x) x + 1 end)(5)
    arrow = (x -> x + 1)(5)
    full == arrow
end

# Top-level full-form IIFE (regression guard) -> 6.
top_level = (function(x) x + 1 end)(5)

ok1 = closures_fullform_iife_basic_8030() == 6
ok2 = closures_fullform_iife_capture_8030() == 15
ok3 = closures_fullform_iife_two_params_8030() == 13
ok4 = closures_fullform_iife_in_if_8030(true) == 6
ok5 = closures_fullform_iife_in_for_8030() == 14      # 1 + 4 + 9
ok6 = closures_fullform_iife_with_macro_8030() == 105
ok7 = closures_fullform_iife_arrow_equiv_8030()
ok8 = top_level == 6

ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && ok7 && ok8
