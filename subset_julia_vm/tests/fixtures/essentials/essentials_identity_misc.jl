using Test

@testset "identity function" begin
    @test identity(42) == 42
    @test identity("hello") == "hello"
    @test identity(true) == true
    @test identity(nothing) === nothing
end

@testset "iszero and isone" begin
    @test iszero(0) == true
    @test iszero(1) == false
    @test iszero(0.0) == true
    @test isone(1) == true
    @test isone(0) == false
    @test isone(1.0) == true
end

@testset "typeof" begin
    @test typeof(42) == Int64
    @test typeof(3.14) == Float64
    @test typeof("hi") == String
    @test typeof(true) == Bool
    @test typeof(nothing) == Nothing
end

true
