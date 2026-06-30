using Test

@testset "array literal construction routes to Array wrapper (#6649)" begin
    a = [1, 2, 3]
    @test typeof(a.ref) == MemoryRef{Int64}
    @test a.size == (3,)
    @test a[2] == 2
    a[3] = 30
    @test a[3] == 30

    empty = Int64[]
    @test typeof(empty.ref) == MemoryRef{Int64}
    @test empty.size == (0,)
    @test length(empty) == 0

    m = [1 2; 3 4]
    @test typeof(m.ref) == MemoryRef{Int64}
    @test m.size == (2, 2)
    @test m[2, 1] == 3
    @test m[1, 2] == 2
end

true
