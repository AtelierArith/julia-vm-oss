using Test

struct DefaultStructEqualityBox3993
    x
end

@testset "default struct equality uses field identity (Issue #3993)" begin
    @test DefaultStructEqualityBox3993(1) == DefaultStructEqualityBox3993(1)

    arr = [1]
    @test DefaultStructEqualityBox3993(arr) == DefaultStructEqualityBox3993(arr)
    @test DefaultStructEqualityBox3993([1]) != DefaultStructEqualityBox3993([1])
    @test [1] == [1]

    m = Memory{Int64}(undef, 1)
    m[1] = 1
    m_alias = m
    m2 = Memory{Int64}(undef, 1)
    m2[1] = 1

    @test DefaultStructEqualityBox3993(m) == DefaultStructEqualityBox3993(m_alias)
    @test DefaultStructEqualityBox3993(m) != DefaultStructEqualityBox3993(m2)
    @test m == m2
end

true
