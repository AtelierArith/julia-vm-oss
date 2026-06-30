using Test

@testset "quoted function parameter splat interpolation (Issue #7522)" begin
    args = [:x]
    ex = :(function f($(args...))
        x
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :f
    @test sig.args[2] == :x

    kwargs = [:(y=2)]
    kw_ex = :(function g($(args...); $(kwargs...))
        x + y
    end)
    kw_sig = kw_ex.args[1]
    params = kw_sig.args[2]
    kw = params.args[1]

    @test kw_ex isa Expr
    @test kw_ex.head == :function
    @test kw_sig.head == :call
    @test kw_sig.args[1] == :g
    @test params.head == :parameters
    @test kw.head == :(=)
    @test kw.args[1] == :y
    @test kw.args[2] == 2
    @test kw_sig.args[3] == :x
end

true
