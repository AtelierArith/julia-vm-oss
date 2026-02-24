# Test insorted argument order fix (Issue #2163)
# Julia's signature: insorted(x, v) — element first, collection second

using Test

@testset "insorted(x, arr) with integers" begin
    arr = [1, 2, 3, 4, 5]
    @test insorted(3, arr) == true
    @test insorted(1, arr) == true
    @test insorted(5, arr) == true
    @test insorted(0, arr) == false
    @test insorted(6, arr) == false
end

@testset "insorted(x, arr) with floats" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    @test insorted(3.0, arr) == true
    @test insorted(1.0, arr) == true
    @test insorted(5.0, arr) == true
    @test insorted(0.0, arr) == false
    @test insorted(6.0, arr) == false
    @test insorted(2.5, arr) == false
end

@testset "insorted edge cases" begin
    # Single element
    @test insorted(1, [1]) == true
    @test insorted(0, [1]) == false

    # Two elements
    @test insorted(1, [1, 2]) == true
    @test insorted(2, [1, 2]) == true
    @test insorted(3, [1, 2]) == false
end

true
