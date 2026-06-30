using Test
using MacroTools

@testset "MacroTools destruct_key handles QuoteNode keys directly (Issue #7637)" begin
    ex = MacroTools.destruct_key(QuoteNode(:a), :tmp, MacroTools.getkeym)
    @test ex isa Expr
    @test ex.head == :call
    @test length(ex.args) == 3
    @test ex.args[2] == :tmp
    @test ex.args[3] == QuoteNode(:a)
end

true
