# Issue #7948: function-form `isdefined(::Module, ::Symbol)` must consult the
# macro binding table so `isdefined(M, Symbol("@name"))` is true for macros that
# are defined/exported/imported and callable — matching upstream Julia.
using Test

module MacroOwner7948
export @owned
macro owned(x)
    return esc(x)
end
# A macro that is NOT exported stays owned by the module but invisible to `using`.
macro hidden(x)
    return esc(x)
end
end

module MacroEmpty7948
end

using .MacroOwner7948

# A macro defined directly at top level (Main).
macro toplevel7948(x)
    return esc(x)
end

@testset "isdefined macro bindings (Issue #7948)" begin
    # Module-owned macro is visible in its owner.
    @test isdefined(MacroOwner7948, Symbol("@owned"))
    @test isdefined(MacroOwner7948, Symbol("@hidden"))

    # Exported macro becomes visible in Main through `using`.
    @test isdefined(Main, Symbol("@owned"))

    # Top-level Main macro is visible in Main.
    @test isdefined(Main, Symbol("@toplevel7948"))

    # Negative cases: unrelated module / nonexistent macro stay false.
    @test !isdefined(MacroEmpty7948, Symbol("@owned"))
    @test !isdefined(MacroOwner7948, Symbol("@nope7948"))
    @test !isdefined(Main, Symbol("@nope7948"))

    # The macro is genuinely callable, confirming this is a reflection-only gap.
    @test (@owned 41) + 1 == 42
end

true
