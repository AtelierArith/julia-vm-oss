using Test

@testset "wrap Array over MemoryRef offset storage" begin
    m = Memory{Int64}(5)
    for i in 1:5
        m[i] = 10 * i
    end

    r = memoryref(m, 3)
    a = wrap(Array, r, 3)
    @test size(a) == (3,)
    @test length(a) == 3
    @test a[1] == 30
    @test a[2] == 40
    a[2] = 99
    @test m[4] == 99

    r2 = memoryref(m, 2)
    b = wrap(Array, r2, (2, 2))
    @test size(b) == (2, 2)
    @test length(b) == 4
    @test b[1, 1] == 20
    @test b[1, 2] == 99
    b[2, 2] = 77
    @test m[5] == 77

    @test_throws DimensionMismatch wrap(Array, r, (4,))
end

true
