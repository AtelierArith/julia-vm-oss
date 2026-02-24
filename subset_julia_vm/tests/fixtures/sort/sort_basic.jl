# Test sort!() and sort() functions (Issue #1879)

using Test

@testset "sort! in-place" begin
    arr = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0]
    sort!(arr)
    @test arr[1] == 1.0
    @test arr[2] == 1.0
    @test arr[3] == 3.0
    @test arr[4] == 4.0
    @test arr[5] == 5.0
    @test arr[6] == 9.0
end

@testset "sort! already sorted" begin
    arr = [1.0, 2.0, 3.0]
    sort!(arr)
    @test arr[1] == 1.0
    @test arr[2] == 2.0
    @test arr[3] == 3.0
end

@testset "sort! reverse order" begin
    arr = [5.0, 4.0, 3.0, 2.0, 1.0]
    sort!(arr)
    @test arr[1] == 1.0
    @test arr[5] == 5.0
end

@testset "sort! single element" begin
    arr = [42.0]
    sort!(arr)
    @test arr[1] == 42.0
end

@testset "sort returns copy" begin
    arr = [3.0, 1.0, 2.0]
    sorted = sort(arr)
    @test sorted[1] == 1.0
    @test sorted[2] == 2.0
    @test sorted[3] == 3.0
    # original unchanged
    @test arr[1] == 3.0
end

true
