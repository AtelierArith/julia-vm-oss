# Issue #8066: an esc'd / interpolated function name used as a call target is the
# OPPOSITE of #8064 — it MUST stay visible to the caller. Covers the short form,
# the full `function ... end` form, an interpolated parameter type, and an
# interpolated (macro-argument) name. Previously these errored "macro expansion
# returned unsupported function callee Expr" (short form) or failed to parse
# ("expected interpolated function name", full form).

# Short form: `$(esc(:f))(x) = body`.
macro def_short()
    quote
        $(esc(:esc_short_fn))(x) = x * 2
    end
end
@def_short
check_short = (esc_short_fn(10) == 20)

# Full `function ... end` form with an interpolated name.
macro def_full()
    quote
        function $(esc(:esc_full_fn))(x)
            x * 2
        end
    end
end
@def_full
check_full = (esc_full_fn(10) == 20)

# Short form with an interpolated parameter type.
macro def_typed(T)
    quote
        $(esc(:esc_typed_fn))(x::$T) = x * 2
    end
end
@def_typed Int
check_typed = (esc_typed_fn(10) == 20)

# Full form whose name comes from a macro argument, `$(esc(fname))`.
macro def_named(fname)
    quote
        function $(esc(fname))(x)
            x + 100
        end
    end
end
@def_named esc_named_fn
check_named = (esc_named_fn(5) == 105)

check_short && check_full && check_typed && check_named
