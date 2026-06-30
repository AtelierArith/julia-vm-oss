using Test
using Iterators

@testset "zip length skips infinite iterators (Issue #4141)" begin
    @test length(zip(1:3, countfrom(10))) == 3
    @test length(zip(countfrom(10), 1:3)) == 3
    @test length(zip(countfrom(1), 1:4, countfrom(10))) == 4
    @test length(zip(countfrom(1), 1:4, countfrom(10), 1:2)) == 2
    @test_throws ArgumentError length(zip(countfrom(1), countfrom(10)))
end

@testset "zip iterator traits follow upstream Julia (Issue #4141)" begin
    @test IteratorSize(zip(1:3, countfrom(10))) isa HasLength
    @test IteratorSize(zip(countfrom(10), 1:3)) isa HasLength
    @test IteratorSize(zip(countfrom(1), countfrom(10))) isa IsInfinite
    @test IteratorEltype(zip(1:3, countfrom(10))) isa HasEltype
    @test eltype(zip(1:3, countfrom(10))) == Tuple{Int64, Int64}
end

@testset "zip collect with infinite finite pair (Issue #4141)" begin
    @test collect(zip(countfrom(10), 1:3)) == [(10, 1), (11, 2), (12, 3)]
end

true
