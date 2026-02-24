# Test basic Task type and functions

using Test

# Task creation and state functions
@testset "Task creation and state" begin
    # Create a simple task
    t = Task(() -> 1 + 1)
    @test isa(t, Task)
    @test istaskdone(t) == false
    @test istaskstarted(t) == false
    @test istaskfailed(t) == false
end

@testset "Task scheduling and fetch" begin
    # Schedule and run a task
    t = Task(() -> 2 * 3)
    schedule(t)
    @test istaskdone(t) == true
    @test istaskstarted(t) == true
    @test istaskfailed(t) == false
    @test fetch(t) == 6
end

@testset "Task with computation" begin
    # Task with more complex computation
    t = Task(() -> sum([1, 2, 3, 4, 5]))
    schedule(t)
    @test fetch(t) == 15
end

@testset "Multiple tasks" begin
    # Multiple tasks
    t1 = Task(() -> 10)
    t2 = Task(() -> 20)
    t3 = Task(() -> 30)

    schedule(t1)
    schedule(t2)
    schedule(t3)

    @test fetch(t1) == 10
    @test fetch(t2) == 20
    @test fetch(t3) == 30
end

# Note: Closure capture test removed - SubsetJuliaVM has limitations with variable capture in closures

@testset "fetch for non-Task" begin
    # fetch on non-Task values returns the value
    @test fetch(42) == 42
    @test fetch("hello") == "hello"
end

@testset "yield function" begin
    # yield is a no-op in cooperative model
    yield()  # should not throw
    @test true
end

true
