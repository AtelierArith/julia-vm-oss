using Test

@testset "Channel{T} basic construction" begin
    c = Channel{Int}(10)
    @test isopen(c)
    @test isempty(c)
    @test !isfull(c)
end

@testset "Channel{T} put! and take!" begin
    c = Channel{Int}(10)
    put!(c, 1)
    put!(c, 2)
    put!(c, 3)
    @test isready(c)
    @test take!(c) == 1
    @test take!(c) == 2
    @test take!(c) == 3
    @test isempty(c)
end

@testset "Channel{Any} explicit type" begin
    c = Channel{Any}(5)
    @test isopen(c)
    put!(c, 42)
    put!(c, "hello")
    @test take!(c) == 42
    @test take!(c) == "hello"
end

@testset "Channel (no type param) still works as Channel{Any}" begin
    c = Channel(5)
    @test isopen(c)
    put!(c, 100)
    @test take!(c) == 100
end

@testset "Channel{Float64} construction and operations" begin
    c = Channel{Float64}(3)
    @test isopen(c)
    put!(c, 1.0)
    put!(c, 2.5)
    @test isready(c)
    @test take!(c) == 1.0
    @test take!(c) == 2.5
end

@testset "Channel{T} close" begin
    c = Channel{Int}(5)
    put!(c, 1)
    close(c)
    @test !isopen(c)
    @test take!(c) == 1
end

@testset "Channel{T} isa Channel" begin
    c = Channel{Int}(5)
    @test isa(c, Channel)
end

true
