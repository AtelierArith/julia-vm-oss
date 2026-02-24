# Test fourthroot() function (Issue #1859)

using Test

@testset "fourthroot positive values" begin
    @test fourthroot(16.0) == 2.0
    @test fourthroot(81.0) == 3.0
    @test fourthroot(256.0) == 4.0
    @test fourthroot(1.0) == 1.0
    @test fourthroot(0.0) == 0.0
end

@testset "fourthroot integer argument" begin
    @test fourthroot(16) == 2.0
    @test fourthroot(81) == 3.0
    @test fourthroot(0) == 0.0
end

true
