using Test

@testset "eval Expr(:curly, ...) (Issue #5930)" begin
    @test eval(Expr(:curly, :Val, 3)) == Val{3}
    @test eval(Meta.parse("Val{3}")) == Val{3}
    @test eval(:(Vector{Int})) == Vector{Int}
end

true
