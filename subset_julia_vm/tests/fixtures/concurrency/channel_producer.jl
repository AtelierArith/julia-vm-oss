using Test

@testset "Channel do-block producer basic" begin
    c = Channel(10) do ch
        put!(ch, 1)
        put!(ch, 2)
        put!(ch, 3)
        put!(ch, 4)
        put!(ch, 5)
    end

    @test isopen(c)
    @test take!(c) == 1
    @test take!(c) == 2
    @test take!(c) == 3
    @test take!(c) == 4
    @test take!(c) == 5
    yield()
    @test !isopen(c)
end

@testset "Channel do-block producer take in order" begin
    c = Channel(10) do ch
        put!(ch, 10)
        put!(ch, 20)
        put!(ch, 30)
    end

    @test take!(c) == 10
    @test take!(c) == 20
    @test take!(c) == 30
    @test isempty(c)
    yield()
    @test !isopen(c)
end

@testset "Channel do-block producer with Inf size" begin
    c = Channel(Inf) do ch
        put!(ch, 1)
        put!(ch, 4)
        put!(ch, 9)
    end

    @test isopen(c)
    @test take!(c) == 1
    @test take!(c) == 4
    @test take!(c) == 9
    yield()
    @test !isopen(c)
end

@testset "Channel do-block producer that errors propagates exception (Issue #3455)" begin
    c = Channel(10) do ch
        put!(ch, 1)
        error("producer error")
    end

    @test take!(c) == 1
    @test_throws TaskFailedException take!(c)
end

@testset "empty! removes all channel items" begin
    c = Channel(10)
    put!(c, 1)
    put!(c, 2)
    put!(c, 3)
    @test !isempty(c)

    empty!(c)
    @test isempty(c) == true
    @test isopen(c) == true
end

true
