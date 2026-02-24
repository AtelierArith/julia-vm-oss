# Test cmp() and isless() comparison functions (Issue #1870)

using Test

@testset "cmp basic" begin
    @test cmp(1, 2) == -1
    @test cmp(2, 1) == 1
    @test cmp(1, 1) == 0
    @test cmp(-1, 1) == -1
    @test cmp(1.0, 2.0) == -1
    @test cmp(2.0, 1.0) == 1
    @test cmp(1.0, 1.0) == 0
end

@testset "isless basic" begin
    @test isless(1, 2) == true
    @test isless(2, 1) == false
    @test isless(1, 1) == false
    @test isless(-1, 0) == true
    @test isless(1.0, 2.0) == true
    @test isless(2.0, 1.0) == false
end

true
