using Test
@testset "empty take of a generator collects without error (Issue #5752)" begin
    # Previously errored "Unbound type parameter: T"; now returns an empty array.
    r = collect(Iterators.take((i for i in 1:5), 0))
    @test isempty(r)
    @test length(r) == 0
    @test r == Int64[]
    @test r == []

    # A mapped generator, also empty
    r2 = collect(Iterators.take((i * 2 for i in 1:3), 0))
    @test isempty(r2)
    @test r2 == Int64[]

    # Non-empty take still works (control)
    @test collect(Iterators.take((i for i in 1:5), 2)) == [1, 2]
    @test collect(Iterators.take((i for i in 1:5), 10)) == [1, 2, 3, 4, 5]
    # Empty take of a range / array already worked (control)
    @test collect(Iterators.take(1:5, 0)) == Int64[]
    @test collect(Iterators.take([1, 2, 3], 0)) == Int64[]
end
true
