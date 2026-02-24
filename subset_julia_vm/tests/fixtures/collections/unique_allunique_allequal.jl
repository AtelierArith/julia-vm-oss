# Test unique, allunique, allequal functions (Issue #1883)

using Test

@testset "unique with duplicates" begin
    arr = [1.0, 2.0, 3.0, 2.0, 1.0]
    u = unique(arr)
    @test length(u) == 3
    @test u[1] == 1.0
    @test u[2] == 2.0
    @test u[3] == 3.0
end

@testset "unique no duplicates" begin
    arr = [1.0, 2.0, 3.0]
    u = unique(arr)
    @test length(u) == 3
end

@testset "unique single element" begin
    arr = [5.0]
    u = unique(arr)
    @test length(u) == 1
    @test u[1] == 5.0
end

@testset "unique all same" begin
    arr = [3.0, 3.0, 3.0]
    u = unique(arr)
    @test length(u) == 1
    @test u[1] == 3.0
end

@testset "allunique true" begin
    @test allunique([1.0, 2.0, 3.0]) == true
end

@testset "allunique false" begin
    @test allunique([1.0, 2.0, 1.0]) == false
end

@testset "allunique single" begin
    @test allunique([5.0]) == true
end

@testset "allequal true" begin
    @test allequal([3.0, 3.0, 3.0]) == true
end

@testset "allequal false" begin
    @test allequal([1.0, 2.0, 3.0]) == false
end

@testset "allequal single" begin
    @test allequal([7.0]) == true
end

@testset "unique preserves original" begin
    arr = [3.0, 1.0, 2.0, 1.0]
    u = unique(arr)
    # original unchanged
    @test arr[1] == 3.0
    @test arr[4] == 1.0
    # result is correct
    @test length(u) == 3
end

true
