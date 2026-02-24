# Curried string search functions: startswith, endswith, contains, occursin
# Issue #2100

using Test

@testset "startswith curried" begin
    f = startswith("he")
    @test f("hello") == true
    @test f("world") == false
    @test f("help") == true
    @test f("") == false
    # 2-arg form still works
    @test startswith("hello", "he") == true
    @test startswith("hello", "wo") == false
end

@testset "endswith curried" begin
    f = endswith("lo")
    @test f("hello") == true
    @test f("world") == false
    @test f("polo") == true
    @test f("") == false
    # 2-arg form still works
    @test endswith("hello", "lo") == true
    @test endswith("hello", "he") == false
end

@testset "contains curried" begin
    f = contains("world")
    @test f("hello world") == true
    @test f("hello") == false
    @test f("worldwide") == true
    # 2-arg form still works
    @test contains("hello world", "world") == true
    @test contains("hello", "xyz") == false
end

@testset "occursin curried" begin
    f = occursin("hello world")
    @test f("world") == true
    @test f("xyz") == false
    @test f("hello") == true
    # 2-arg form still works
    @test occursin("world", "hello world") == true
    @test occursin("xyz", "hello") == false
end

true
