# Issue #10194: a struct definition nested inside the body a user-defined
# macro is asked to expand (via `esc(body)` interpolation, not literally
# written inside the macro's own `quote`) must lower and run — the same
# "transparent block" rule as `@testset`/`let`/`begin`. Here the struct
# lives inside the caller-supplied `body` argument, which `Macro10194`
# re-emits verbatim inside its own `let ... end` wrapper.

macro wrap_in_let_10194(body)
    quote
        let
            $(esc(body))
        end
    end
end

result = @wrap_in_let_10194 begin
    struct FooUserMacroLet10194
        x::Int
    end
    FooUserMacroLet10194(7).x
end

result == 7
