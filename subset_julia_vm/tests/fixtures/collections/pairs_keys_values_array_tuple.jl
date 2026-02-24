# Test pairs(), keys(), values() for arrays and tuples (Issue #1872)

using Test

@testset "keys for array" begin
    arr = [10, 20, 30]
    k = keys(arr)
    @test k == 1:3
end

@testset "values for array" begin
    arr = [10, 20, 30]
    v = values(arr)
    @test v == [10, 20, 30]
end

@testset "pairs for array" begin
    arr = [10, 20, 30]
    p = pairs(arr)
    @test p[1] == (1, 10)
    @test p[2] == (2, 20)
    @test p[3] == (3, 30)
end

@testset "keys for tuple" begin
    t = (10, 20, 30)
    k = keys(t)
    @test k == 1:3
end

@testset "values for tuple" begin
    t = (10, 20, 30)
    v = values(t)
    @test v == (10, 20, 30)
end

@testset "pairs for tuple" begin
    t = (10, 20, 30)
    p = pairs(t)
    @test p[1] == (1, 10)
    @test p[2] == (2, 20)
    @test p[3] == (3, 30)
end

true
