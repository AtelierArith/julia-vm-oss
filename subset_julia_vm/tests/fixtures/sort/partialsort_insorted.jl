# Test partialsort, insorted, reverse! functions (Issue #1879)

using Test

@testset "partialsort basic" begin
    @test partialsort([5.0, 3.0, 1.0, 4.0, 2.0], 1) == 1.0
    @test partialsort([5.0, 3.0, 1.0, 4.0, 2.0], 3) == 3.0
    @test partialsort([5.0, 3.0, 1.0, 4.0, 2.0], 5) == 5.0
end

@testset "partialsort! modifies array" begin
    arr = [5.0, 3.0, 1.0, 4.0, 2.0]
    result = partialsort!(arr, 2)
    @test result == 2.0
    # First 2 elements should be the 2 smallest (sorted)
    @test arr[1] == 1.0
    @test arr[2] == 2.0
end

@testset "insorted found" begin
    sorted = [1.0, 2.0, 3.0, 4.0, 5.0]
    @test insorted(3.0, sorted) == true
    @test insorted(1.0, sorted) == true
    @test insorted(5.0, sorted) == true
end

@testset "insorted not found" begin
    sorted = [1.0, 2.0, 3.0, 4.0, 5.0]
    @test insorted(0.0, sorted) == false
    @test insorted(6.0, sorted) == false
    @test insorted(2.5, sorted) == false
end

@testset "reverse! basic" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    reverse!(arr)
    @test arr[1] == 5.0
    @test arr[2] == 4.0
    @test arr[3] == 3.0
    @test arr[4] == 2.0
    @test arr[5] == 1.0
end

@testset "reverse! single" begin
    arr = [42.0]
    reverse!(arr)
    @test arr[1] == 42.0
end

@testset "reverse! two elements" begin
    arr = [1.0, 2.0]
    reverse!(arr)
    @test arr[1] == 2.0
    @test arr[2] == 1.0
end

true
