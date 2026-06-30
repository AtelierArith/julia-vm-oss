# Issue #7933: a short-form function definition whose parameter type comes from an
# interpolation inside a `quote` — e.g. AbstractAlgebra's `@attributes`-style
# `f(x::$T) = ...` — failed to LOWER. The macro expansion round-trips it as
# `Expr(:(=), Expr(:call, f, Expr(:(::), x, <spliced type>)), body)`; the
# macro-result→IR converter had no `:call`-target case for `Expr(:(=), ...)`, so it
# fell through to the assignment-expression path and errored with
# `UnsupportedFeature { MacroCall, "macro expansion returned unsupported assignment
# expression target" }`. Upstream Julia lowers it fine. The fix routes a `:call`
# (and `:where`) assignment LHS to the same function-definition builder used for the
# full `Expr(:function, ...)` form.
#
# The checks below observe the definition by calling the function *inside* the same
# quote. Each macro uses a distinct function name on purpose: macro-defined names in
# a quote currently leak into a shared top-level binding in sjulia instead of being
# gensym'd (Issue #8064), so reusing one name across macros would merge their method
# tables. Related, independent gaps surfaced here: `local f(x) = ...` short forms in a
# quote (Issue #8065) and esc'd call targets `$(esc(:f))(x) = ...` (Issue #8066).

# 1. The exact #7933 reproduction shape: defining the function must not crash the
#    pipeline. We only assert the program lowers and runs to completion.
macro define_only(T)
    quote
        define_only_fn(x::$T) = x
        :defined
    end
end
check_define = (@define_only Int) === :defined

# 2. Interpolated typed parameter, observed by calling the function in the same
#    quote.
macro addone(T)
    quote
        addone_fn(x::$T) = x + 1
        addone_fn(41)
    end
end
check_addone = (@addone Int) == 42

# 3. Multiple parameters mixing an interpolated and a literal type annotation.
macro combine(T)
    quote
        combine_fn(a::$T, b::Int) = a * 10 + b
        combine_fn(4, 2)
    end
end
check_combine = (@combine Int) == 42

# 4. A `where` clause alongside an interpolated parameter type. The type parameters
#    must be preserved, not dropped.
macro addwhere(T)
    quote
        addwhere_fn(x::$T, y::S) where {S<:Real} = x + y
        addwhere_fn(7, 3)
    end
end
check_where = (@addwhere Int) == 10

# 5. Untyped and interpolated-typed parameters in the same expansion.
macro mixparams(T)
    quote
        mix_typed(x::$T) = x * 2
        mix_untyped(y) = y + 100
        mix_typed(21) + mix_untyped(0)
    end
end
check_mix = (@mixparams Int) == 142

# 6. The interpolated annotation becomes the *real* declared type: dispatch picks
#    the typed method for a matching argument and the fallback otherwise.
macro twomethods(T)
    quote
        twom_fn(x::$T) = "typed"
        twom_fn(x) = "fallback"
        (twom_fn(3), twom_fn("hi"))
    end
end
check_dispatch = (@twomethods Int) == ("typed", "fallback")

# 7. The interpolated type is honored for a non-numeric type too.
macro onlystr(T)
    quote
        onlystr_fn(x::$T) = "got string"
        onlystr_fn("hi")
    end
end
check_string = (@onlystr String) == "got string"

check_define && check_addone && check_combine && check_where &&
    check_mix && check_dispatch && check_string
