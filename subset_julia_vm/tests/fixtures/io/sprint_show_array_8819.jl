using Test

@testset "sprint(show, array) uses compact array display (Issue #8819)" begin
    d = Dict(:x => 10)
    @test sprint(show, collect(pairs(d))) == "[:x => 10]"
    @test sprint(show, [1, 2]) == "[1, 2]"
    @test sprint(show, [true, false]) == "Bool[1, 0]"
    @test sprint(show, Any[1, 2]) == "Any[1, 2]"
end

true
