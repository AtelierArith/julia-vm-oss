using Test

@testset "quoted assignment with typed LHS (Issue #7622)" begin
    ex = :(x::Int = nothing)
    @test ex isa Expr
    @test ex.head === :(=)

    lhs = ex.args[1]
    @test lhs isa Expr
    @test lhs.head === :(::)
    @test lhs.args[1] === :x
    @test lhs.args[2] === :Int
    @test ex.args[2] === :nothing
end

@testset "plain quoted assignment LHS remains a Symbol (Issue #7622)" begin
    ex = :(x = nothing)
    @test ex isa Expr
    @test ex.head === :(=)
    @test ex.args[1] === :x
    @test ex.args[2] === :nothing
end

true
