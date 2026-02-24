# Test ndims and length for scalar Number types (Issue #2171)
# Julia: ndims(x::Number) = 0, length(x::Number) = 1

using Test

@testset "ndims for integers" begin
    @test ndims(42) == 0
    @test ndims(Int64(0)) == 0
    @test ndims(-10) == 0
end

@testset "ndims for floats" begin
    @test ndims(3.14) == 0
    @test ndims(0.0) == 0
    @test ndims(Float32(1.5)) == 0
end

@testset "ndims for Bool" begin
    @test ndims(true) == 0
    @test ndims(false) == 0
end

@testset "ndims for arrays (regression)" begin
    @test ndims([1, 2, 3]) == 1
    @test ndims([1 2; 3 4]) == 2
end

@testset "length for integers" begin
    @test length(42) == 1
    @test length(Int64(0)) == 1
    @test length(-10) == 1
end

@testset "length for floats" begin
    @test length(3.14) == 1
    @test length(0.0) == 1
    @test length(Float32(1.5)) == 1
end

@testset "length for Bool" begin
    @test length(true) == 1
    @test length(false) == 1
end

@testset "length for arrays (regression)" begin
    @test length([1, 2, 3]) == 3
    @test length([1 2; 3 4]) == 4
end

true
