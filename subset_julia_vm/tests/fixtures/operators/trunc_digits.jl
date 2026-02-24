# trunc() with digits and sigdigits keyword arguments (Issue #2059)

using Test

@testset "trunc with digits keyword" begin
    @test trunc(3.14159, digits=2) == 3.14
    @test trunc(3.14159, digits=4) == 3.1415
    @test trunc(3.14159, digits=0) == 3.0
    @test trunc(-1.7, digits=0) == -1.0
    @test trunc(100.0, digits=2) == 100.0
end

@testset "trunc with sigdigits keyword" begin
    @test trunc(3.14159, sigdigits=3) == 3.14
    @test trunc(3.14159, sigdigits=1) == 3.0
    @test trunc(1234.0, sigdigits=2) == 1200.0
end

@testset "trunc default (no keywords)" begin
    @test trunc(3.7) == 3.0
    @test trunc(-1.7) == -1.0
    @test trunc(0.0) == 0.0
end

true
