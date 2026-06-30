using Test

@testset "quoted semicolon block interpolation (Issue #7511)" begin
    line = nothing
    yes = :(1)
    ex = :($line;$yes)

    @test ex isa Expr
    @test ex.head == :block
    @test nothing in ex.args
    @test 1 in ex.args
end

true
