using Test

macro capture(ex)
    QuoteNode(ex)
end

@testset "var-string identifiers are macro argument Symbols" begin
    ex = @capture var"@q", var"@qq", postwalk
    @test ex == Expr(:tuple, Symbol("@q"), Symbol("@qq"), :postwalk)
    @test string(ex) == "(var\"@q\", var\"@qq\", postwalk)"
    @test sprint(print, ex) == "(var\"@q\", var\"@qq\", postwalk)"
end

true
