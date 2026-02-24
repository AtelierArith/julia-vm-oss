# Test isassigned(array, i) for index assignment check (Issue #1836)
# In SubsetJuliaVM, all isbits array elements are always assigned,
# so isassigned is effectively a bounds check.

using Test

@testset "isassigned basic" begin
    arr = [10, 20, 30, 40, 50]

    # Valid indices return true
    @test isassigned(arr, 1) == true
    @test isassigned(arr, 3) == true
    @test isassigned(arr, 5) == true

    # Out of bounds return false
    @test isassigned(arr, 0) == false
    @test isassigned(arr, 6) == false
    @test isassigned(arr, -1) == false
end

@testset "isassigned Float64 array" begin
    arr = [1.0, 2.0, 3.0]

    @test isassigned(arr, 1) == true
    @test isassigned(arr, 3) == true
    @test isassigned(arr, 4) == false
end

@testset "isassigned single element" begin
    arr = [42]
    @test isassigned(arr, 1) == true
    @test isassigned(arr, 2) == false
end

@testset "isassigned empty array" begin
    arr = Int64[]
    @test isassigned(arr, 1) == false
end

true
