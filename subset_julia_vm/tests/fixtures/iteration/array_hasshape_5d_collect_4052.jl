using Test

@testset "5D Array HasShape collect path (Issue #4052)" begin
    source = reshape(collect(1:32), 2, 2, 2, 2, 2)
    trait = Base.IteratorSize(source)
    @test typeof(trait) === Base.HasShape{5}

    values = Base._collect(1:1, source, Base.IteratorEltype(source), trait)
    @test typeof(values) === Array{Int64,5}
    @test eltype(values) === Int64
    @test ndims(values) == 5
    @test size(values) == (2, 2, 2, 2, 2)
    @test values[1, 1, 1, 1, 1] == 1
    @test values[2, 2, 2, 2, 2] == 32

    values[1, 1, 1, 1, 1] = 99
    @test values[1, 1, 1, 1, 1] == 99
    @test source[1, 1, 1, 1, 1] == 1
end

true
