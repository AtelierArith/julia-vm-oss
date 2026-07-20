using Test

@testset "SubArray and Vector structural equality (Issue #10296)" begin
    xs = [1, 2, 3, 4, 5]
    v = view(xs, 1:2)

    @test v == [1, 2]
    @test [1, 2] == v
    @test [v] == [[1, 2]]
    @test [[1, 2]] == [v]

    chunks = collect(Iterators.partition(xs, 2))
    @test chunks == [[1, 2], [3, 4], [5]]
    @test [[1, 2], [3, 4], [5]] == chunks
    @test chunks[1] == [1, 2]
    @test chunks[3] == [5]
end

true
