using Test

module MacroModuleLiteralExport7955

export target

macro mirror_alias(alias_name::Symbol, real_name::Symbol)
    return esc(quote
        const $alias_name = $real_name
        if $(QuoteNode(real_name)) in names($__module__)
            export $alias_name
        end
    end)
end

target() = 41
private_target() = 42

@mirror_alias alias_target target
@mirror_alias alias_private_target private_target

module Nested

export nested_target

macro mirror_alias(alias_name::Symbol, real_name::Symbol)
    return esc(quote
        const $alias_name = $real_name
        if $(QuoteNode(real_name)) in names($__module__)
            export $alias_name
        end
    end)
end

nested_target() = 43
nested_private_target() = 44

@mirror_alias alias_nested_target nested_target
@mirror_alias alias_nested_private_target nested_private_target

end

end

using .MacroModuleLiteralExport7955

@testset "macro module literal export mirroring (PR #7955)" begin
    module_names = names(MacroModuleLiteralExport7955)
    @test :target in module_names
    @test :alias_target in module_names
    @test !(:private_target in module_names)
    @test !(:alias_private_target in module_names)
    @test isdefined(MacroModuleLiteralExport7955, :alias_target)
    @test isdefined(MacroModuleLiteralExport7955, :alias_private_target)

    nested_names = names(MacroModuleLiteralExport7955.Nested)
    @test :Nested in nested_names
    @test !(Symbol("MacroModuleLiteralExport7955.Nested") in nested_names)
    @test :nested_target in nested_names
    @test :alias_nested_target in nested_names
    @test !(:nested_private_target in nested_names)
    @test !(:alias_nested_private_target in nested_names)
    @test isdefined(MacroModuleLiteralExport7955.Nested, :alias_nested_target)
    @test isdefined(MacroModuleLiteralExport7955.Nested, :alias_nested_private_target)
end

true
