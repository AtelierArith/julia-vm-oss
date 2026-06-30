using Test

@testset "bind closes channel on task success" begin
    c = Channel(10)
    t = Task(() -> begin
        put!(c, 1)
        put!(c, 2)
    end)
    bind(c, t)
    schedule(t)

    @test isopen(c) == false
    @test length(c) == 2
    @test take!(c) == 1
    @test take!(c) == 2
end

@testset "bind closes channel with TaskFailedException on task failure" begin
    c = Channel(10)
    t = Task(() -> begin
        put!(c, 99)
        error("task failed")
    end)
    bind(c, t)
    schedule(t)

    @test isopen(c) == false
    @test istaskfailed(t) == true
    @test take!(c) == 99
end

@testset "bind on already completed task closes channel immediately" begin
    c = Channel(10)
    t = Task(() -> 42)
    schedule(t)
    @test istaskdone(t) == true

    bind(c, t)
    @test isopen(c) == false
end

@testset "bind on already failed task closes channel with exception" begin
    c = Channel(10)
    t = Task(() -> error("oops"))
    schedule(t)
    @test istaskfailed(t) == true

    bind(c, t)
    @test isopen(c) == false
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

    @test isopen(c1) == false
    @test isopen(c2) == false
    @test take!(c1) == :a
    @test take!(c2) == :b
end

true
