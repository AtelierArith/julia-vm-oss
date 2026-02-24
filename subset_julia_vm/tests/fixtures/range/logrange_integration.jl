# Test LogRange integration with comprehensions and HOFs (Issue #1835)
# Verify that LogRange works correctly with higher-order functions and comprehensions

using Test

@testset "LogRange comprehension" begin
    result = [x for x in logrange(1, 100, 3)]
    @test length(result) == 3
    @test result[1] == 1.0
    @test result[3] == 100.0
    @test abs(result[2] - 10.0) < 1e-10
end

@testset "LogRange map" begin
    r = logrange(1.0, 100.0, 3)
    doubled = map(x -> 2 * x, r)
    @test length(doubled) == 3
    @test doubled[1] == 2.0
    @test doubled[3] == 200.0
end

@testset "LogRange filter" begin
    r = logrange(1.0, 1000.0, 4)
    # Values are approximately 1, 10, 100, 1000
    big = filter(x -> x > 50.0, r)
    @test length(big) == 2
end

@testset "LogRange sum via reduce" begin
    r = logrange(1.0, 100.0, 3)
    # sum of 1.0 + 10.0 + 100.0 = 111.0
    s = sum(r)
    @test abs(s - 111.0) < 1e-10
end

@testset "LogRange eachindex and firstindex/lastindex" begin
    r = logrange(1.0, 100.0, 5)
    @test firstindex(r) == 1
    @test lastindex(r) == 5
    idx = eachindex(r)
    @test first(idx) == 1
    @test last(idx) == 5
end

true
