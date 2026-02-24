# Test LogRange basic operations (Issue #1833)
# LogRange provides lazy logarithmically-spaced values between start and stop

using Test

@testset "LogRange basic operations" begin
    r = logrange(1.0, 100.0, 5)

    # Length
    @test length(r) == 5

    # First and last (exact endpoints)
    @test first(r) == 1.0
    @test last(r) == 100.0
end

@testset "LogRange getindex" begin
    r = logrange(1.0, 100.0, 3)

    # Endpoints are exact
    @test r[1] == 1.0
    @test r[3] == 100.0

    # Middle value: geometric mean of 1 and 100 = 10.0
    @test abs(r[2] - 10.0) < 1e-10
end

@testset "LogRange iteration" begin
    r = logrange(1.0, 1000.0, 4)
    values = collect(r)

    @test length(values) == 4
    @test values[1] == 1.0
    @test values[4] == 1000.0

    # Intermediate values: 1, ~10, ~100, 1000 (powers of 10)
    @test abs(values[2] - 10.0) < 1e-10
    @test abs(values[3] - 100.0) < 1e-10
end

@testset "LogRange single element" begin
    r = logrange(5.0, 5.0, 1)
    @test length(r) == 1
    @test first(r) == 5.0
    @test r[1] == 5.0
end

@testset "LogRange two elements" begin
    r = logrange(2.0, 8.0, 2)
    @test length(r) == 2
    @test r[1] == 2.0
    @test r[2] == 8.0
end

@testset "LogRange empty" begin
    r = logrange(1.0, 10.0, 0)
    @test length(r) == 0
    @test isempty(r)
end

true
