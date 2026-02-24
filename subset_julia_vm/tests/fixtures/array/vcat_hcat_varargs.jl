# Test vcat/hcat with 3+ arguments (Issue #2169)
# Julia supports varargs: vcat(a, b, c, ...) and hcat(a, b, c, ...)

using Test

@testset "vcat with 3 arguments" begin
    r = vcat([1, 2], [3, 4], [5, 6])
    @test length(r) == 6
    @test r[1] == 1
    @test r[3] == 3
    @test r[5] == 5
    @test r[6] == 6
end

@testset "vcat with 4 arguments" begin
    r = vcat([1], [2], [3], [4])
    @test length(r) == 4
    @test r[1] == 1
    @test r[4] == 4
end

@testset "vcat with 2 arguments (regression)" begin
    r = vcat([1, 2], [3, 4])
    @test length(r) == 4
    @test r[1] == 1
    @test r[4] == 4
end

@testset "hcat with 3 arguments" begin
    r = hcat([1, 2], [3, 4], [5, 6])
    @test size(r) == (2, 3)
    @test r[1, 1] == 1.0
    @test r[2, 1] == 2.0
    @test r[1, 3] == 5.0
    @test r[2, 3] == 6.0
end

@testset "hcat with 4 arguments" begin
    r = hcat([1, 2], [3, 4], [5, 6], [7, 8])
    @test size(r) == (2, 4)
    @test r[1, 1] == 1.0
    @test r[1, 4] == 7.0
    @test r[2, 4] == 8.0
end

@testset "hcat with 2 arguments (regression)" begin
    r = hcat([1, 2], [3, 4])
    @test size(r) == (2, 2)
    @test r[1, 1] == 1.0
    @test r[2, 2] == 4.0
end

true
