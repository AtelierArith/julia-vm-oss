# Test basic Channel type and functions
# Note: SubsetJuliaVM Channel is simplified (no type parameters)

using Test

@testset "Channel creation" begin
    # Create buffered channels
    ch1 = Channel(10)
    @test isa(ch1, Channel)
    @test isopen(ch1) == true
    @test isempty(ch1) == true
    @test isready(ch1) == false
    @test isbuffered(ch1) == true
end

@testset "Channel put! and take!" begin
    ch = Channel(10)

    # Put items
    put!(ch, 1)
    put!(ch, 2)
    put!(ch, 3)

    @test isready(ch) == true
    @test length(ch) == 3

    # Take items (FIFO order)
    @test take!(ch) == 1
    @test take!(ch) == 2
    @test take!(ch) == 3

    @test isempty(ch) == true
end

@testset "Channel isfull" begin
    ch = Channel(2)

    @test isfull(ch) == false
    put!(ch, 1)
    @test isfull(ch) == false
    put!(ch, 2)
    @test isfull(ch) == true
end

@testset "Channel close" begin
    ch = Channel(10)
    put!(ch, 42)

    @test isopen(ch) == true
    close(ch)
    @test isopen(ch) == false

    # Can still take from closed channel with data
    @test take!(ch) == 42
end

@testset "Channel fetch" begin
    ch = Channel(10)
    put!(ch, 100)

    # fetch returns first item without removal
    @test fetch(ch) == 100
    @test length(ch) == 1  # item still there

    # take! removes the item
    @test take!(ch) == 100
    @test length(ch) == 0
end

true
