using Test

v = VersionNumber(1, 2, 3)

@testset "VersionNumber construction" begin
    @test v.major == 1
    @test v.minor == 2
    @test v.patch == 3
end

v2 = VersionNumber(4, 5)

@testset "VersionNumber 2-arg constructor" begin
    @test v2.major == 4
    @test v2.minor == 5
    @test v2.patch == 0
end

v3 = VersionNumber(7)

@testset "VersionNumber 1-arg constructor" begin
    @test v3.major == 7
    @test v3.minor == 0
    @test v3.patch == 0
end

@testset "VERSION constant" begin
    @test isa(VERSION, VersionNumber)
    @test VERSION.major >= 0
end

true
