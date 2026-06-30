using Test

@testset "Expr print renders QuoteNode Symbol payloads as quoted symbols" begin
    ex = :(Dict(:a => 1))
    @test string(ex) == "Dict(:a => 1)"
    @test sprint(print, ex) == "Dict(:a => 1)"

    non_symbol = Expr(:call, :f, QuoteNode(1))
    @test string(non_symbol) == "f(\$(QuoteNode(1)))"
end

true
