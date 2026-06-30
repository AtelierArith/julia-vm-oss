using Test

function generator_local_roundtrip_5081()
    g = Base.Generator(x -> x + 1, [1, 2, 3])
    return collect(g)
end

function generator_local_reassign_5081()
    g = Base.Generator(x -> x + 1, [1])
    g = 42
    return g
end

@testset "generator local carrier consolidation (Issue #5081)" begin
    @test generator_local_roundtrip_5081() == [2, 3, 4]
    @test generator_local_reassign_5081() == 42
end

true
