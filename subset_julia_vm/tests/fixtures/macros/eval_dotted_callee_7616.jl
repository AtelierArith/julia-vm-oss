using Test
using MacroTools

@testset "eval supports Expr dotted callees (Issue #7616)" begin
    global eval_dotted_callee_7616_ex = :(foo(1))
    body = Expr(
        :call,
        Expr(:., :MacroTools, QuoteNode(:trymatch)),
        Expr(:quote, :(foo(x_))),
        :eval_dotted_callee_7616_ex,
    )
    env = eval(body)
    @test env isa Dict
    @test env[:x] == 1
end

true
