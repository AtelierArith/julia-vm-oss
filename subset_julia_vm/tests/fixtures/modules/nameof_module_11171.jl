# nameof(::Module) reflection support (Issue #11171)
#
# Upstream: nameof(m::Module) = ccall(:jl_module_name, ...) returns the
# module's own unqualified binding name as a Symbol, regardless of how deeply
# nested the module is or how it was reached (bare reference, dynamic
# dispatch, or a ::Module-annotated argument).

using Test

module NameofModuleOuter11171
module NameofModuleInner11171 end
end

g(m) = nameof(m)
h(m::Module) = nameof(m)

@testset "nameof(::Module)" begin
    @test nameof(NameofModuleOuter11171.NameofModuleInner11171) === :NameofModuleInner11171
    @test nameof(NameofModuleOuter11171) === :NameofModuleOuter11171
    @test g(NameofModuleOuter11171.NameofModuleInner11171) === :NameofModuleInner11171
    @test h(NameofModuleOuter11171.NameofModuleInner11171) === :NameofModuleInner11171
    @test nameof(Main) === :Main
    @test nameof(Base) === :Base
    @test nameof(NameofModuleOuter11171.NameofModuleInner11171) isa Symbol
end

true
