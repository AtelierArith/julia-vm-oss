# Test range types have size/length interface methods (Issue #2706)
# Verifies AbstractArray-like interface for range types

using Test

@testset "UnitRange interface" begin
    r = 1:10
    @test length(r) == 10
    @test first(r) == 1
    @test last(r) == 10
end

@testset "StepRange interface" begin
    r = 1:2:9
    @test length(r) == 5
    @test first(r) == 1
    @test last(r) == 9
end

@testset "LinRange interface" begin
    r = LinRange(1.0, 10.0, 5)
    @test length(r) == 5
    @test size(r) == (5,)
    @test first(r) == 1.0
    @test last(r) == 10.0
end

@testset "StepRangeLen interface" begin
    r = StepRangeLen(0.0, 0.25, 5)
    @test length(r) == 5
    @test size(r) == (5,)
    @test first(r) == 0.0
end

true
