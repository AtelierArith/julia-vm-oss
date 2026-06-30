using Test

@testset "wrap Array range and colon indexing" begin
    m = Memory{Int64}(5)
    for i in 1:5
        m[i] = 10 * i
    end

    a = wrap(Array, m, 5)
    r = a[2:4]
    @test size(r) == (3,)
    @test r[1] == 20
    @test r[2] == 30
    @test r[3] == 40

    r[2] = 999
    @test a[3] == 30
    @test m[3] == 30

    c = a[:]
    @test size(c) == (5,)
    @test c[1] == 10
    @test c[5] == 50

    a[1] = 7.0
    @test a[1] == 7
end

true
