using Test

module ModuleNames7938
export x
x = 1
y = 2
end

@testset "names(::Module) exposes default exported bindings (Issue #7938)" begin
    module_names = names(ModuleNames7938)
    @test module_names isa Vector{Symbol}
    @test :ModuleNames7938 in module_names
    @test :x in module_names
    @test !(:y in module_names)

    bare_names = names
    @test bare_names(ModuleNames7938) == module_names

    qualified_names = Base.names
    @test qualified_names(ModuleNames7938) == module_names
end

true
