using Test

@testset "quoted let expression (Issue #7512)" begin
    ex = :(let x = 1
        x
    end)
    binding = ex.args[1]
    body = ex.args[2]

    @test ex isa Expr
    @test ex.head == :let
    @test binding isa Expr
    @test binding.head == :(=)
    @test binding.args[1] == :x
    @test binding.args[2] == 1
    @test body isa Expr
    @test body.head == :block
end

true
