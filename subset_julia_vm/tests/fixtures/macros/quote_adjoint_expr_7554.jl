using Test

@testset "quote construction supports adjoint expressions (Issue #7554)" begin
    ex = :(x')

    @test ex isa Expr
    @test ex.head == Symbol("'")
    @test length(ex.args) == 1
    @test ex.args[1] == :x
end

true
