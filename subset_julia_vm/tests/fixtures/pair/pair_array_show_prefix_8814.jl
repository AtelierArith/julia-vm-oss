using Test

@testset "Pair and composite array show prefixes" begin
    d = Dict(:x => 10)

    @test repr(collect(pairs(d))) == "[:x => 10]"
    @test repr([1 => 2]) == "[1 => 2]"
    @test repr([(a = 1, b = :x)]) == "[(a = 1, b = :x)]"
end

true
