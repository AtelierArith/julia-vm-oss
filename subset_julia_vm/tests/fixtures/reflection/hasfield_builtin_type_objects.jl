# Test hasfield for builtin runtime type objects

using Test

@testset "hasfield - builtin runtime type objects" begin
    @test hasfield(LineNumberNode, :line)
    @test hasfield(LineNumberNode, :file)
    @test !hasfield(LineNumberNode, :missing)

    @test hasfield(Expr, :head)
    @test hasfield(Expr, :args)
    @test !hasfield(Expr, :missing)

    @test hasfield(QuoteNode, :value)
    @test !hasfield(QuoteNode, :args)

    @test hasfield(GlobalRef, :mod)
    @test hasfield(GlobalRef, :name)
    @test hasfield(GlobalRef, :binding)
    @test !hasfield(GlobalRef, :missing)
end

true
