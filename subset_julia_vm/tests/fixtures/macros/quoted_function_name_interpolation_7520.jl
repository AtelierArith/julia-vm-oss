using Test

@testset "quoted function name interpolation (Issue #7520)" begin
    fname = :f
    ex = :(function $fname(x)
        x
    end)
    sig = ex.args[1]

    @test ex isa Expr
    @test ex.head == :function
    @test sig isa Expr
    @test sig.head == :call
    @test sig.args[1] == :f
    @test sig.args[2] == :x

    args = [:x]
    kwargs = [:(y=2)]
    combined = :(function $fname($(args...); $(kwargs...))
        x + y
    end)
    combined_sig = combined.args[1]
    params = combined_sig.args[2]
    kw = params.args[1]

    @test combined isa Expr
    @test combined.head == :function
    @test combined_sig.head == :call
    @test combined_sig.args[1] == :f
    @test params.head == :parameters
    @test kw.head == :(=)
    @test kw.args[1] == :y
    @test combined_sig.args[3] == :x
end

true
