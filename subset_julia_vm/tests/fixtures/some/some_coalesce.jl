using Test

@testset "Some with isa" begin
    s = Some(42)
    @test isa(s, Some)
    @test s.value == 42
    s2 = Some("hello")
    @test s2.value == "hello"
end

@testset "something with nested nothings" begin
    @test something(nothing, 10) == 10
    @test something(nothing, nothing, 30) == 30
    @test something(5) == 5
    @test something(Some(5)) == 5
end

@testset "isnothing" begin
    @test isnothing(nothing) == true
    @test isnothing(42) == false
    @test isnothing("") == false
    @test isnothing(0) == false
end

@testset "Some wrapping nothing" begin
    s = Some(nothing)
    @test s.value === nothing
    @test isnothing(s) == false
end

true
