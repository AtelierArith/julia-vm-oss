using Test

@testset "Memory equality and hash direct storage" begin
    m = Memory{Int64}(undef, 3)
    n = Memory{Int64}(undef, 3)

    for i in 1:3
        m[i] = i
        n[i] = i
    end

    a = [1, 2, 3]
    f = [1.0, 2.0, 3.0]
    mat = reshape([1, 2, 3], 1, 3)

    @test isequal(m, n)

    @test m == a
    @test a == m
    @test m == f
    @test !(m == mat)
    @test !(m === n)
    @test m === m

    @test hash(m) == hash(a)
    @test hash(m) == hash(n)

    n[3] = 30
    @test !isequal(m, n)
    @test !(m == n)
    @test hash(m) != hash(n)
end

true
