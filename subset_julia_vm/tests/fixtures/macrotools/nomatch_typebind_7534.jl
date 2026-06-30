using Test
using MacroTools

@testset "MacroTools TypeBind @nomatch fallback lowers (Issue #7534)" begin
    bind = MacroTools.TypeBind(:x, Set{Any}([:call]))

    miss = MacroTools.match_inner(bind, :notcall, Dict{Symbol,Any}())
    @test miss isa MacroTools.MatchError
    @test miss.ex == :notcall
end

true
