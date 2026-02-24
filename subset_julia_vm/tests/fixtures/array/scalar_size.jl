# Test size(::Number) returns empty tuple for scalars (Issue #2179)
# Julia: size(::Number) = (), consistent with ndims(::Number) = 0

using Test

@testset "size(::Number) returns empty tuple" begin
    @test size(42) == ()
    @test size(3.14) == ()
    @test size(true) == ()
    @test size(Float32(1.0)) == ()
end

@testset "size(::Number) length is 0" begin
    @test length(size(42)) == 0
    @test length(size(3.14)) == 0
end

@testset "size for arrays (regression)" begin
    @test size([1, 2, 3]) == (3,)
    @test size([1 2; 3 4]) == (2, 2)
    @test size([1, 2, 3], 1) == 3
end

true
