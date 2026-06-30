# Test pairs(), keys(), values() for arrays and tuples (Issue #1872)

using Test

@testset "keys for array" begin
    arr = [10, 20, 30]
    k = keys(arr)
    @test length(k) == 3
    key_sum = 0
    for i in k
        key_sum = key_sum + i
    end
    @test key_sum == 6
end

@testset "values for array" begin
    arr = [10, 20, 30]
    v = values(arr)
    @test v == [10, 20, 30]
end

@testset "pairs for array" begin
    arr = [10, 20, 30]
    p = pairs(arr)
    @test occursin("Pairs", string(typeof(p)))
    @test p[1] == 10
    @test p[2] == 20
    @test p[3] == 30
    @test length(p) == 3
    @test length(keys(p)) == 3
    @test values(p) === arr

    first_pair = iterate(p)[1]
    @test first_pair.first == 1
    @test first_pair.second == 10
end

@testset "keys for tuple" begin
    t = (10, 20, 30)
    k = keys(t)
    @test length(k) == 3
    key_sum = 0
    for i in k
        key_sum = key_sum + i
    end
    @test key_sum == 6
end

@testset "values for tuple" begin
    t = (10, 20, 30)
    v = values(t)
    @test v == (10, 20, 30)
end

@testset "pairs for tuple" begin
    t = (10, 20, 30)
    p = pairs(t)
    @test occursin("Pairs", string(typeof(p)))
    @test p[1] == 10
    @test p[2] == 20
    @test p[3] == 30
    @test values(p) === t

    first_pair = iterate(p)[1]
    @test first_pair.first == 1
    @test first_pair.second == 10
end

true
