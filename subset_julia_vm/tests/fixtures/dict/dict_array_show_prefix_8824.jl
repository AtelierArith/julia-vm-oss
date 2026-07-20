using Test

@testset "Dict array show prefix" begin
    @test repr([Dict(:x => 1)]) == "[Dict(:x => 1)]"
end

true
