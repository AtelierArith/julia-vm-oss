using Test

# Tests for cooperative blocking put!/take!/fetch via pending_puts queue (Issue #3451)

@testset "put! on full buffered channel does not throw" begin
    ch = Channel(1)
    put!(ch, 1)
    @test isfull(ch)
    put!(ch, 2)  # buffer full — goes to pending_puts, must not throw
    @test isfull(ch)    # buffer still reports full
    @test isready(ch)   # value is available via pending queue
    @test !isempty(ch)
end

@testset "take! drains pending puts in FIFO order" begin
    ch = Channel(1)
    put!(ch, 1)
    put!(ch, 2)  # pending
    put!(ch, 3)  # pending
    @test take!(ch) == 1
    @test take!(ch) == 2
    @test take!(ch) == 3
    @test isempty(ch)
end

@testset "multiple pending puts maintain FIFO order" begin
    ch = Channel(2)
    put!(ch, 10)
    put!(ch, 20)
    put!(ch, 30)  # pending
    put!(ch, 40)  # pending
    @test take!(ch) == 10
    @test take!(ch) == 20
    @test take!(ch) == 30
    @test take!(ch) == 40
    @test isempty(ch)
end

@testset "isempty false when pending puts exist" begin
    ch = Channel(1)
    put!(ch, 1)
    put!(ch, 2)  # pending
    @test !isempty(ch)
    take!(ch)
    @test !isempty(ch)  # 2 was drained from pending to buffer
    take!(ch)
    @test isempty(ch)
end

@testset "isready true when pending puts exist" begin
    ch = Channel(1)
    put!(ch, 1)
    put!(ch, 2)  # pending
    @test isready(ch)
    take!(ch)
    @test isready(ch)  # 2 drained to buffer
    take!(ch)
    @test !isready(ch)
end

@testset "@async producer with more items than capacity" begin
    ch = Channel(2)
    t = @async begin
        for i in 1:5
            put!(ch, i)
        end
    end
    @test istaskdone(t)
    results = Int[]
    for _ in 1:5
        push!(results, take!(ch))
    end
    @test results == [1, 2, 3, 4, 5]
end

@testset "unbuffered channel: second put goes to pending" begin
    ch = Channel(0)
    put!(ch, 42)
    put!(ch, 99)  # second put goes to pending
    @test take!(ch) == 42
    @test take!(ch) == 99
    @test isempty(ch)
end

@testset "empty! clears pending puts too" begin
    ch = Channel(1)
    put!(ch, 1)
    put!(ch, 2)  # pending
    empty!(ch)
    @test isempty(ch)
    @test length(ch) == 0
    @test !isready(ch)
end

@testset "close does not lose pending items" begin
    ch = Channel(1)
    put!(ch, 1)
    put!(ch, 2)  # pending
    close(ch)
    @test !isopen(ch)
    @test take!(ch) == 1  # from buffer
    @test take!(ch) == 2  # drained from pending after take
end

true
