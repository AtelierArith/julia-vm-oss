using Test

# VM-level task continuation and Channel blocking parity (Issues #10349 / #10144).
# Every case runs unchanged under upstream Julia: no main-task overflow without
# a consumer, and no sjulia-only pending overflow queue.

@testset "buffered put! suspends at capacity and resumes after take!" begin
    c = Channel(1)
    events = Int[]
    t = @async begin
        put!(c, 1)
        put!(c, 2)
        push!(events, 99)
    end
    yield()
    @test isready(c)
    @test !istaskdone(t)
    push!(events, take!(c))
    push!(events, 0)
    push!(events, take!(c))
    wait(t)
    @test events == [1, 0, 99, 2]
end

@testset "buffered producer maintains FIFO across repeated suspension" begin
    c = Channel(2)
    t = @async begin
        for i in 1:5
            put!(c, i)
        end
    end
    values = Int[]
    for _ in 1:5
        push!(values, take!(c))
    end
    wait(t)
    @test values == [1, 2, 3, 4, 5]
    @test isempty(c)
    @test istaskdone(t)
end

@testset "multiple blocked puts preserve producer order" begin
    c = Channel(1)
    t = @async begin
        put!(c, 10)
        put!(c, 20)
        put!(c, 30)
        put!(c, 40)
    end
    @test [take!(c) for _ in 1:4] == [10, 20, 30, 40]
    wait(t)
end

@testset "empty!/isready observe only available buffered values" begin
    c = Channel(1)
    t = @async begin
        put!(c, 1)
        put!(c, 2)
    end
    yield()
    @test isready(c)
    empty!(c)
    wait(t)
    @test isready(c)
    @test take!(c) == 2
    @test !isready(c)
end

@testset "unbuffered Channel is a producer/consumer rendezvous" begin
    c = Channel(0)
    events = String[]
    t = @async begin
        push!(events, "before")
        put!(c, 42)
        push!(events, "after")
    end
    yield()
    @test events == ["before"]
    push!(events, "main")
    @test take!(c) == 42
    wait(t)
    @test events == ["before", "main", "after"]
end

@testset "multiple unbuffered producers hand off one rendezvous at a time" begin
    c = Channel(0)
    events = Int[]
    first = @async begin
        put!(c, 10)
        push!(events, 1)
    end
    second = @async begin
        put!(c, 20)
        push!(events, 2)
    end

    yield()
    @test take!(c) == 10
    yield()
    @test events == [1]
    @test take!(c) == 20
    yield()
    @test events == [1, 2]
    wait(first)
    wait(second)
end

@testset "empty take! parks until a scheduled producer supplies a value" begin
    c = Channel(1)
    t = @async begin
        yield()
        put!(c, 7)
    end
    @test take!(c) == 7
    wait(t)
end

@testset "close preserves already-buffered values" begin
    c = Channel(2)
    put!(c, 1)
    put!(c, 2)
    close(c)
    @test !isopen(c)
    @test take!(c) == 1
    @test take!(c) == 2
end

@testset "yield suspends at the yield point" begin
    events = String[]
    t = @async begin
        push!(events, "task1")
        yield()
        push!(events, "task2")
    end
    yield()
    push!(events, "main")
    wait(t)
    @test events == ["task1", "main", "task2"]
end

@testset "wait parks one task while another task runs" begin
    events = String[]
    child = @async begin
        push!(events, "child1")
        yield()
        push!(events, "child2")
        17
    end
    waiter = @async begin
        push!(events, "waiter1")
        wait(child)
        push!(events, "waiter2")
    end
    wait(waiter)
    @test fetch(child) == 17
    @test events == ["child1", "waiter1", "child2", "waiter2"]
end

@testset "Condition wait/notify resumes parked continuations" begin
    condition = Condition()
    events = Any[]
    t = @async begin
        push!(events, "waiting")
        value = wait(condition)
        push!(events, value)
    end
    yield()
    @test notify(condition, 7) == 1
    wait(t)
    @test events == Any["waiting", 7]
end

@testset "sleep is a cooperative timer yield point" begin
    events = String[]
    t = @async begin
        sleep(0.01)
        push!(events, "task")
    end
    yield()
    push!(events, "main")
    wait(t)
    @test events == ["main", "task"]
end

@testset "Channel do-block producer runs on a live task" begin
    c = Channel(1) do ch
        put!(ch, 5)
        put!(ch, 6)
    end
    @test take!(c) == 5
    @test take!(c) == 6
end

@testset "waitany and waitall drive real task sets" begin
    first = @async begin
        yield()
        1
    end
    second = @async 2
    done, remaining = waitany([first, second])
    @test !isempty(done)
    all_done, all_remaining = waitall([first, second])
    @test length(all_done) == 2
    @test isempty(all_remaining)
end

mutable struct SuspendedTaskBox10349
    value::Int
end

@testset "errormonitor observes a failure that happens after registration" begin
    monitored = @async error("monitored task failure")
    @test errormonitor(monitored) === monitored
    yield()
    @test istaskfailed(monitored)
end

@testset "suspended task frames remain GC roots" begin
    answer = Ref(0)
    t = @async begin
        box = SuspendedTaskBox10349(41)
        yield()
        answer[] = box.value + 1
    end
    yield()
    GC.gc()
    wait(t)
    @test answer[] == 42
end

true
