# Regression test for Issues #8113 and #8114: accessing a member through a
# `Module` VALUE — either a nested sub-module (`Outer.Inner.T1`, #8113) or a
# `const` bound to a module (`const MA = Mod1; MA.S`, #8114) — must resolve the
# member inside that module. Before the fix the qualified field-access / call
# path treated the intermediate `Module` value as a struct and failed with
# "Field access requires a struct type, got Module" (nested, compile time) or
# "GetFieldByName: expected struct, got Module" (const alias, runtime). The
# member may be a type, a function, or a const, and the chain may be 2+ levels
# deep, including an alias-rooted nested chain (`const AO = Outer; AO.Inner.T1`).
using Test

module Outer8113
    module Inner
        struct T1 end
        deep_fn() = 42
        const KONST = 7
        module Deeper
            struct U end
            g() = 100
        end
    end
end

module Mod1_8114
    struct S end
    f() = 99
    const C = 5
end

const MA = Mod1_8114
const AO = Outer8113

# A local parameter named exactly like a module must shadow it (Issue #7245):
# `Mod1_8114.val` inside this body is struct field access on the parameter, not
# module-qualified access on the module of the same name.
struct ShadowHolder8113
    val::Int
end
field_via_module_named_param(Mod1_8114::ShadowHolder8113) = Mod1_8114.val

@testset "member access through a Module value (Issues #8113, #8114)" begin
    # #8113 — nested sub-module: type, function, const members.
    @test typeof(Outer8113.Inner.T1) === DataType
    @test Outer8113.Inner.T1 isa Type
    @test Outer8113.Inner.T1 === Outer8113.Inner.T1
    @test Outer8113.Inner.deep_fn() == 42
    @test Outer8113.Inner.KONST == 7

    # #8113 — three levels deep (type + function).
    @test typeof(Outer8113.Inner.Deeper.U) === DataType
    @test Outer8113.Inner.Deeper.g() == 100

    # #8114 — const bound to a module: type, function, const members.
    @test typeof(MA.S) === DataType
    @test MA.S isa Type
    @test MA.f() == 99
    @test MA.C == 5
    # The alias resolves to the SAME type object as the direct qualified name.
    @test MA.S === Mod1_8114.S

    # Combined: alias-rooted nested chain resolves like the direct chain.
    @test AO.Inner.T1 === Outer8113.Inner.T1
    @test AO.Inner.deep_fn() == 42
    @test AO.Inner.Deeper.U === Outer8113.Inner.Deeper.U
    @test AO.Inner.Deeper.g() == 100

    # Regression (Issue #7245): a parameter named like a module shadows it, so
    # the field access resolves to the struct field, not the module.
    @test field_via_module_named_param(ShadowHolder8113(13)) == 13
end

true
