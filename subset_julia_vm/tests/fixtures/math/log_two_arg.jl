# Test log(base, x) two-argument form (Issue #2175)
# Julia: log(b, x) = log(x) / log(b)

using Test

@testset "log(base, x) basic" begin
    @test log(2.0, 8.0) == 3.0
    @test log(2, 8) == 3.0
    @test log(10.0, 100.0) == 2.0
    @test log(10.0, 1000.0) ≈ 3.0
end

@testset "log(base, x) edge cases" begin
    @test log(2.0, 1.0) == 0.0
    @test log(10.0, 1.0) == 0.0
    @test log(2.0, 2.0) == 1.0
    @test log(3.0, 27.0) == 3.0
end

@testset "log(x) single argument (regression)" begin
    @test log(1.0) == 0.0
    @test log(ℯ) ≈ 1.0
    @test log(ℯ^2) ≈ 2.0
end

true
