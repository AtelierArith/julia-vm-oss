using Test
using MacroTools

@testset "MacroTools allbindings handles QuoteNode.value guard (Issue #7535)" begin
    bs = Any[]
    MacroTools.allbindings(QuoteNode(:x_), bs)
    @test bs == Any[:x]

    quoted_literal = QuoteNode(:literal)
    @test quoted_literal.value === :literal
end

true
