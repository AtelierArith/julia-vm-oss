using Test

# Channel blocking, producer failure, and `isfull` parity regressions.

@testset "concurrency_channel_bugfixes_buffered_put_parks (Issues #3454, #10349)" begin
    ch = Channel(1)
    put!(ch, 1)
    events = Int[]
    producer = @async begin
        push!(events, 10)
        put!(ch, 2)
        push!(events, 20)
    end

    yield()
    @test events == [10]
    @test take!(ch) == 1
    @test events == [10]
    yield()
    @test events == [10, 20]
    @test take!(ch) == 2
    wait(producer)
end

@testset "concurrency_channel_bugfixes_producer_exception_propagates (Issue #3455)" begin
    ch = Channel(10) do ch
        error("something went wrong")
    end
    @test_throws TaskFailedException take!(ch)
end

@testset "concurrency_channel_bugfixes_isfull_unbuffered_empty (Issue #3456)" begin
    ch = Channel(0)
    @test isfull(ch) == true
end

@testset "concurrency_channel_bugfixes_unbuffered_rendezvous (Issues #3456, #10349)" begin
    ch = Channel(0)
    events = Int[]
    producer = @async begin
        push!(events, 1)
        put!(ch, 10)
        push!(events, 2)
    end

    yield()
    @test events == [1]
    @test isfull(ch) == true
    @test isready(ch)
    @test take!(ch) == 10
    yield()
    @test events == [1, 2]
    wait(producer)
end

@testset "concurrency_channel_bugfixes_isfull_buffered_unchanged (Issue #3456)" begin
    ch = Channel(2)
    @test isfull(ch) == false
    put!(ch, 1)
    @test isfull(ch) == false
    put!(ch, 2)
    @test isfull(ch) == true
end

true
