using Test

@testset "quoted docstrings lower to Core.@doc macrocall (Issue #7712)" begin
    ex = quote
        "doc"
        f(x) = x
    end
    @test length(ex.args) == 2
    doccall = ex.args[2]
    @test doccall isa Expr
    @test doccall.head == :macrocall
    @test doccall.args[1] == GlobalRef(Core, Symbol("@doc"))
    @test doccall.args[3] == "doc"
    @test doccall.args[4].head == :(=)

    semicolon = quote; "doc"; g(x) = x; end
    no_lines = Any[]
    for arg in semicolon.args
        if !(arg isa LineNumberNode)
            push!(no_lines, arg)
        end
    end
    @test no_lines[1] == "doc"
    @test no_lines[2].head == :(=)
end

true
