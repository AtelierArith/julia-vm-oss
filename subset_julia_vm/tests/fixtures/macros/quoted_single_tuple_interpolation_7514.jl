using Test

@testset "quoted single tuple interpolation (Issue #7514)" begin
    arg = :x
    ex = :($arg,)

    @test ex isa Expr
    @test ex.head == :tuple
    @test length(ex.args) == 1
    @test ex.args[1] == :x
end

true
