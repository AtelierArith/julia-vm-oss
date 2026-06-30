using Test
using MacroTools: flatten, striplines

@testset "eval Expr(:try) else branch value" begin
    @test eval(:(try 1 + 1 catch; false; else 234; finally end)) == 234
    @test eval(:(try error() catch; 123 else 234 finally end)) == 123
    caught = eval(:(try error() catch; 123 else 234 finally end))
    @test caught == 123
    @test eval(flatten(striplines(:(try 1 + 1 catch; false; else 234; finally end)))) == 234
    @test eval(flatten(:(try 1 + 1 catch; false; else 234; finally end))) == 234
end

true
