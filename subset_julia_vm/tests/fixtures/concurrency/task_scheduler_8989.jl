# Task scheduler regression coverage for Issue #8989.

using Test

@testset "@async schedules before running" begin
    r = Int[]
    t = @async push!(r, 2)
    push!(r, 1)
    @test r == [1]
    @test !istaskdone(t)
    wait(t)
    @test r == [1, 2]
    @test istaskdone(t)
end

@testset "yield runs a scheduled task" begin
    r = Int[]
    t = Task(() -> push!(r, 3))
    schedule(t)
    @test r == Int[]
    yield()
    @test r == [3]
    @test istaskdone(t)
end

@testset "empty channel take drives scheduled producer" begin
    c = Channel(1)
    t = @async put!(c, 42)
    @test take!(c) == 42
    wait(t)
    @test istaskdone(t)
    @test isempty(c)
end

true
