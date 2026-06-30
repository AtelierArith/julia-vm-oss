using Test

@testset "Quoted typed expressions construct Expr(:(::), ...) (Issue #7537)" begin
    ex = :(x::Int)

    @test ex isa Expr
    @test ex.head == :(::)
    @test ex.args[1] == :x
    @test ex.args[2] == :Int
end

true
