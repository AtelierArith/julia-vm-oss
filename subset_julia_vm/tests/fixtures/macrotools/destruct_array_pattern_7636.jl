using Test
using MacroTools

@testset "MacroTools @destruct captures array patterns (Issue #7636)" begin
    d = @destruct [a, b] = Dict(:a => 1, :b => 2)
    @test d == Dict(:a => 1, :b => 2)
    @test (a, b) == (1, 2)

    @destruct [single] = Dict("single" => "ok")
    @test single == "ok"
end

true
