# Test range() with keyword argument forms (Issue #2167)
# Julia's range() supports multiple call patterns with keyword arguments

using Test

@testset "range(start, stop, length) - positional" begin
    r = range(0, 1, 5)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 0.0
    @test c[5] == 1.0
end

@testset "range(start, stop; length=N)" begin
    r = range(0, 1, length=5)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 0.0
    @test c[3] == 0.5
    @test c[5] == 1.0
end

@testset "range(start; stop=S, length=N)" begin
    r = range(0, stop=1, length=5)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 0.0
    @test c[5] == 1.0
end

@testset "range(start; step=S, length=N)" begin
    r = range(1, step=2, length=5)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 1.0
    @test c[2] == 3.0
    @test c[5] == 9.0
end

@testset "range(start; length=N) - UnitRange" begin
    r = range(1, length=5)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 1
    @test c[5] == 5
end

@testset "range(start, stop; step=S)" begin
    r = range(1, 10, step=2)
    c = collect(r)
    @test length(c) == 5
    @test c[1] == 1
    @test c[5] == 9
end

true
