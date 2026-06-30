using Test

@testset "4D Array HasShape collect path (Issue #4091)" begin
    source = reshape(collect(1:16), 2, 2, 2, 2)
    trait = Base.IteratorSize(source)
    @test typeof(trait) === Base.HasShape{4}

    values = Base._collect(1:1, source, Base.IteratorEltype(source), trait)
    @test typeof(values) === Array{Int64,4}
    @test eltype(values) === Int64
    @test ndims(values) == 4
    @test size(values) == (2, 2, 2, 2)
    @test values[1, 1, 1, 1] == 1
    @test values[2, 2, 2, 2] == 16

    values[1, 1, 1, 1] = 99
    @test values[1, 1, 1, 1] == 99
    @test source[1, 1, 1, 1] == 1
end

true
