# Variadic max/min with 3+ arguments (Issue #2135)
# Verifies that max and min reduce pairwise for any number of arguments.

using Test

@testset "max with 3 arguments (Issue #2135)" begin
    @test max(1, 2, 3) == 3
    @test max(3, 2, 1) == 3
    @test max(1, 3, 2) == 3
end

@testset "min with 3 arguments (Issue #2135)" begin
    @test min(1, 2, 3) == 1
    @test min(3, 2, 1) == 1
    @test min(2, 1, 3) == 1
end

@testset "max with 4+ arguments (Issue #2135)" begin
    @test max(1, 2, 3, 4) == 4
    @test max(4, 3, 2, 1) == 4
    @test max(1, 4, 2, 3) == 4
    @test max(1, 2, 3, 4, 5) == 5
end

@testset "min with 4+ arguments (Issue #2135)" begin
    @test min(1, 2, 3, 4) == 1
    @test min(4, 3, 2, 1) == 1
    @test min(2, 1, 4, 3) == 1
    @test min(5, 4, 3, 2, 1) == 1
end

@testset "max/min variadic with negative values (Issue #2135)" begin
    @test max(-1, -2, -3) == -1
    @test min(-1, -2, -3) == -3
    @test max(-5, 0, 5) == 5
    @test min(-5, 0, 5) == -5
end

@testset "max/min variadic with floats (Issue #2135)" begin
    @test max(1.0, 2.5, 3.0) == 3.0
    @test min(1.0, 2.5, 0.5) == 0.5
end

true
