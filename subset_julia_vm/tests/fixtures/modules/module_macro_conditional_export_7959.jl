using Test

# Issue #7959: a module-level `export` inside a *macro-expanded* top-level block
# (a bare `quote ... end`, not wrapped in `esc`) must record into the module's
# exports, exactly like a direct source-level `export`.
#
# Before the fix, a macro that returned a bare `quote ... end` block whose
# trailing element was an `export` (or an `if ... export ... end`) was routed
# through `expand_macro_to_stmt`'s value-producing expression path. On that path
# `Expr(:export, ...)` lowers to a bare `nothing` literal, so the export effect
# was silently dropped and `collect_module_body_exports` never saw a
# `Stmt::Export`. The `esc(quote ...)` variant (PR #7955) accidentally worked
# because its top-level value is an `Expr(:escape, ...)`, not a block, and so
# took the statement path. This fixture pins the bare-`quote` variant.

module MacroExportPlain7959

macro mkexport()
    quote
        export y
    end
end

@mkexport
y = 2

end

module MacroExportConditional7959

macro mkexport_if()
    quote
        if true
            export shown
        end
        if false
            export hidden
        end
    end
end

@mkexport_if
shown = 10
hidden = 20

end

using .MacroExportPlain7959
using .MacroExportConditional7959

@testset "macro-expanded conditional module exports (Issue #7959)" begin
    plain_names = names(MacroExportPlain7959)
    @test :y in plain_names
    @test y == 2

    cond_names = names(MacroExportConditional7959)
    @test :shown in cond_names
    @test !(:hidden in cond_names)
    @test shown == 10
end

true
