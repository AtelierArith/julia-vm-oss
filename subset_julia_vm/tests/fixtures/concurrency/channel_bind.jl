using Test

@testset "bind closes channel on task success" begin
    c = Channel(10)
    t = Task(() -> begin
        put!(c, 1)
        put!(c, 2)
    end)
    bind(c, t)
    schedule(t)
    wait(t)

    @test isopen(c) == false
    @test take!(c) == 1
    @test take!(c) == 2
end

@testset "bind on already completed task closes channel immediately" begin
    c = Channel(10)
    t = Task(() -> 42)
    schedule(t)
    wait(t)
    @test istaskdone(t) == true

    @test bind(c, t) === c
end

@testset "multiple channels bound to one task" begin
    c1 = Channel(10)
    c2 = Channel(10)
    t = Task(() -> begin
        put!(c1, :a)
        put!(c2, :b)
    end)
    bind(c1, t)
    bind(c2, t)
    schedule(t)
    wait(t)

    @test isopen(c1) == false
    @test isopen(c2) == false
    v1 = take!(c1)
    v2 = take!(c2)
    @test v1 == :a
    @test v2 == :b
end

true
