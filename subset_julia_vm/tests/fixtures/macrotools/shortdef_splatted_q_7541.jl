using Test
using MacroTools

@testset "MacroTools shortdef typed branches avoid nested @q splat failure (Issue #7541)" begin
    ex = :(function f(x)::Int
        x + 1
    end)
    short = MacroTools.shortdef(ex)

    @test short.head == :(=)
    @test short.args[1] == Expr(:(::), Expr(:call, :f, :x), :Int)
    @test short.args[2] isa Expr
    @test short.args[2].head == :block
end

true
