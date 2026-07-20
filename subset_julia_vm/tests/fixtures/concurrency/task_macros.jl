# Test task macros: @task, @async, @sync

using Test

@testset "@task macro" begin
    t = @task 1 + 2
    @test isa(t, Task)
    @test istaskstarted(t) == false
    @test istaskdone(t) == false

    schedule(t)
    @test istaskstarted(t) == false
    @test istaskdone(t) == false
    wait(t)
    @test istaskstarted(t) == true
    @test istaskdone(t) == true
    @test fetch(t) == 3
end

@testset "@async macro" begin
    t = @async 2 * 3
    @test isa(t, Task)
    @test istaskstarted(t) == false
    @test istaskdone(t) == false
    wait(t)
    @test istaskstarted(t) == true
    @test istaskdone(t) == true
    @test istaskfailed(t) == false
    @test fetch(t) == 6
end

@testset "@sync with @async side effects" begin
    x = 0
    @sync begin
        @async begin
            x = 10
        end
    end
    @test x == 10
end

@testset "@sync with assigned @async success" begin
    t = nothing
    @sync begin
        t = @async 21
    end

    @test isa(t, Task)
    @test istaskdone(t)
    @test fetch(t) == 21
end

@testset "@async failed task" begin
    t = @async error("boom")
    @test isa(t, Task)
    wait_threw = false
    try
        wait(t)
    catch e
        wait_threw = isa(e, TaskFailedException)
    end
    @test wait_threw
    @test istaskdone(t) == true
    @test istaskfailed(t) == true
    @test_throws TaskFailedException fetch(t)
end

@testset "@sync aggregates standalone @async failures" begin
    ex = nothing
    try
        @sync begin
            @async error("first")
            @async error("second")
        end
    catch e
        ex = e
    end

    @test isa(ex, CompositeException)
    @test length(ex) == 2
end

@testset "@sync aggregates assigned @async failures" begin
    t = nothing
    ex = nothing
    try
        @sync begin
            t = @async error("assigned")
        end
    catch e
        ex = e
    end

    @test isa(t, Task)
    @test istaskfailed(t)
    @test isa(ex, CompositeException)
    @test length(ex) == 1
end

@testset "@sync aggregates single @async expression" begin
    ex = nothing
    try
        @sync @async error("single")
    catch e
        ex = e
    end

    @test isa(ex, CompositeException)
    @test length(ex) == 1
end

true
