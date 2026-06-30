using Test

# Bug fixes for Channel: #3454 (length), #3455 (exception propagation), #3456 (isfull unbuffered)

@testset "concurrency_channel_bugfixes_length_counts_pending (Issue #3454)" begin
    ch = Channel(1)
    put!(ch, 1)    # data=[1], pending=[]
    put!(ch, 2)    # data=[1], pending=[2]
    @test length(ch) == 2
    @test isempty(ch) == false
    take!(ch)
    @test length(ch) == 1
    take!(ch)
    @test length(ch) == 0
end

@testset "concurrency_channel_bugfixes_length_unbuffered_pending (Issue #3454)" begin
    ch = Channel(0)
    put!(ch, 10)
    put!(ch, 20)
    @test length(ch) == 2
end

@testset "concurrency_channel_bugfixes_producer_exception_propagates (Issue #3455)" begin
    @test_throws ErrorException Channel(10) do ch
        error("something went wrong")
    end
end

@testset "concurrency_channel_bugfixes_isfull_unbuffered_empty (Issue #3456)" begin
    ch = Channel(0)
    @test isfull(ch) == false
end

@testset "concurrency_channel_bugfixes_isfull_unbuffered_after_put (Issue #3456)" begin
    ch = Channel(0)
    put!(ch, 1)
    @test isfull(ch) == true
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
