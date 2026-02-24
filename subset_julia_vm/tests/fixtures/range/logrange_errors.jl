# Test LogRange error cases (Issue #1835)
# Verify that invalid inputs produce appropriate errors

using Test

@testset "LogRange negative start" begin
    @test_throws ErrorException logrange(-1.0, 10.0, 3)
end

@testset "LogRange zero start" begin
    @test_throws ErrorException logrange(0.0, 10.0, 3)
end

@testset "LogRange negative stop" begin
    @test_throws ErrorException logrange(1.0, -10.0, 3)
end

@testset "LogRange zero stop" begin
    @test_throws ErrorException logrange(1.0, 0.0, 3)
end

@testset "LogRange negative length" begin
    @test_throws ErrorException logrange(1.0, 10.0, -1)
end

@testset "LogRange endpoints differ with length=1" begin
    @test_throws ErrorException logrange(1.0, 10.0, 1)
end

true
