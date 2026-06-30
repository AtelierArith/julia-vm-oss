using Test

@testset "Quoted typed where function signatures lower to Expr(:function) (Issue #7540)" begin
    ex = :(function f(x::T) where T
        x
    end)

    @test ex isa Expr
    @test ex.head == :function
    @test length(ex.args) == 2
    @test ex.args[1] isa Expr
    @test ex.args[1].head == :where
end

true
