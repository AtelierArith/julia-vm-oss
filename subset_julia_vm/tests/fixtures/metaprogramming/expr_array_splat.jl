using Test

@testset "Expr constructor Array splat expansion" begin
    args = [:+, 1, 2]
    ex = Expr(:call, args...)

    @test ex.head === :call
    @test length(ex.args) == 3
    @test ex.args[1] === :+
    @test ex.args[2] == 1
    @test ex.args[3] == 2
end

true
