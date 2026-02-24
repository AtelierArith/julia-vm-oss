# floor() and ceil() with digits and sigdigits keyword arguments (Issue #2054)

using Test

@testset "floor with digits keyword" begin
    @test floor(3.14159, digits=2) == 3.14
    @test floor(3.14159, digits=4) == 3.1415
    @test floor(3.14159, digits=0) == 3.0
    @test floor(-1.7, digits=0) == -2.0
    @test floor(100.0, digits=2) == 100.0
end

@testset "ceil with digits keyword" begin
    @test ceil(3.14159, digits=2) == 3.15
    @test ceil(3.14159, digits=4) == 3.1416
    @test ceil(3.14159, digits=0) == 4.0
    @test ceil(-1.7, digits=0) == -1.0
    @test ceil(100.0, digits=2) == 100.0
end

@testset "floor with sigdigits keyword" begin
    @test floor(3.14159, sigdigits=3) == 3.14
    @test floor(3.14159, sigdigits=1) == 3.0
    @test floor(1234.0, sigdigits=2) == 1200.0
end

@testset "ceil with sigdigits keyword" begin
    @test ceil(3.14159, sigdigits=3) == 3.15
    @test ceil(3.14159, sigdigits=1) == 4.0
    @test ceil(1234.0, sigdigits=2) == 1300.0
end

@testset "floor and ceil default (no keywords)" begin
    @test floor(3.7) == 3.0
    @test ceil(3.2) == 4.0
    @test floor(-1.2) == -2.0
    @test ceil(-1.7) == -1.0
end

true
